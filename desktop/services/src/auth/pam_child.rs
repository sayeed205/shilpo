//! The PAM conversation, run only inside a freshly `fork`+`exec`'d child process
//! (triggered by the `SHILPO_PAM_HELPER` environment variable — see
//! `main.rs`'s dispatch at process startup, before any Tokio runtime exists).
//!
//! PAM modules are third-party C code (`pam_exec` can run arbitrary scripts) that can
//! crash or call `exit()`. A locker process where that happens can leave the session
//! permanently locked per the `ext-session-lock-v1` protocol, so PAM never runs
//! in-process. It is also never reached via a raw `libc::fork()` from the (multi-threaded,
//! Tokio-based) domain owner: `fork()` without an immediate `exec()` only clones the
//! calling thread, leaving any locks held by other threads at fork time locked forever in
//! the child. Re-executing ourselves via `std::process::Command` (fork+exec) is what makes
//! this safe, and mirrors the existing `SHILPO_WASM_VALIDATOR` self-reexec pattern already
//! used by this binary.
//!
//! Protocol on stdout (one line per event, matching the shape already established for
//! `polkit-agent-helper-1` in `polkit/helper.rs`):
//!   `PAM_PROMPT_ECHO_OFF <prompt>` — masked prompt; a response line is read from stdin.
//!   `PAM_PROMPT_ECHO_ON <prompt>`  — visible prompt; a response line is read from stdin.
//!   `PAM_ERROR_MSG <text>`         — supplementary error text; no response expected.
//!   `PAM_TEXT_INFO <text>`         — supplementary info text; no response expected.
//!   `SUCCESS`                      — terminal: authentication succeeded.
//!   `FAILURE <message>`            — terminal: authentication failed.

use std::ffi::{CStr, CString};
use std::io::{self, BufRead, Write};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

use pam_sys::{
    PAM_AUTHINFO_UNAVAIL, PAM_CONV_ERR, PAM_ERROR_MSG, PAM_PROMPT_ECHO_OFF, PAM_PROMPT_ECHO_ON,
    PAM_SUCCESS, PAM_TEXT_INFO, pam_authenticate, pam_conv, pam_end, pam_message, pam_response,
    pam_start, pam_strerror,
};

fn resolve_current_username() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let mut buf = vec![0i8; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = ptr::null_mut();

    loop {
        let rc = unsafe {
            libc::getpwuid_r(
                uid,
                &mut pwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut result as *mut _,
            )
        };
        if rc == 0 && !result.is_null() {
            let name = unsafe { CStr::from_ptr(pwd.pw_name) };
            return name.to_str().ok().map(|s| s.to_string());
        }
        if rc != libc::ERANGE {
            return None;
        }
        if buf.len() > 1 << 20 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

/// Writes one protocol line to stdout, flushing immediately so the parent's reader thread
/// observes it without buffering delay.
fn emit_line(line: &str) {
    let mut stdout = io::stdout();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

/// Blocks reading one line from stdin (the parent's response to a prompt), trimming the
/// trailing newline. Returns an empty string on EOF/error rather than blocking forever.
fn read_response_line() -> String {
    let mut line = String::new();
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    match lock.read_line(&mut line) {
        Ok(0) | Err(_) => String::new(),
        Ok(_) => line.trim_end_matches(['\r', '\n']).to_string(),
    }
}

/// The `pam_conv` callback. Safety: PAM guarantees `num_msg >= 0`, `msg` points to
/// `num_msg` valid `*const pam_message` entries, and the caller (libpam) frees whatever
/// `*resp` is set to with the C allocator — so response strings are allocated with
/// `libc::strdup`/`libc::calloc`, never the Rust global allocator.
unsafe extern "C" fn conversation_callback(
    num_msg: c_int,
    msg: *mut *const pam_message,
    resp: *mut *mut pam_response,
    _appdata_ptr: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() {
        return PAM_CONV_ERR;
    }
    let count = num_msg as usize;

    let replies =
        unsafe { libc::calloc(count, std::mem::size_of::<pam_response>()) as *mut pam_response };
    if replies.is_null() {
        return pam_sys::PAM_BUF_ERR;
    }

    for i in 0..count {
        let message_ptr = unsafe { *msg.add(i) };
        if message_ptr.is_null() {
            unsafe { free_partial_replies(replies, i) };
            return PAM_CONV_ERR;
        }
        let message = unsafe { &*message_ptr };
        let text = if message.msg.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(message.msg) }
                .to_string_lossy()
                .into_owned()
        };

        let reply = unsafe { &mut *replies.add(i) };
        reply.resp = ptr::null_mut();
        reply.resp_retcode = 0;

        match message.msg_style {
            s if s == PAM_PROMPT_ECHO_OFF => {
                emit_line(&format!("PAM_PROMPT_ECHO_OFF {text}"));
                let response = read_response_line();
                reply.resp = strdup_response(&response);
            }
            s if s == PAM_PROMPT_ECHO_ON => {
                emit_line(&format!("PAM_PROMPT_ECHO_ON {text}"));
                let response = read_response_line();
                reply.resp = strdup_response(&response);
            }
            s if s == PAM_ERROR_MSG => {
                emit_line(&format!("PAM_ERROR_MSG {text}"));
            }
            s if s == PAM_TEXT_INFO => {
                emit_line(&format!("PAM_TEXT_INFO {text}"));
            }
            _ => {
                unsafe { free_partial_replies(replies, i + 1) };
                return PAM_CONV_ERR;
            }
        }
    }

    unsafe {
        *resp = replies;
    }
    PAM_SUCCESS
}

fn strdup_response(response: &str) -> *mut c_char {
    let Ok(c_response) = CString::new(response) else {
        return ptr::null_mut();
    };
    unsafe { libc::strdup(c_response.as_ptr()) }
}

/// Frees the first `count` response entries' `resp` strings and the array itself, for
/// cleanup on an error path before returning control to libpam.
unsafe fn free_partial_replies(replies: *mut pam_response, count: usize) {
    for i in 0..count {
        let reply = unsafe { &*replies.add(i) };
        if !reply.resp.is_null() {
            unsafe { libc::free(reply.resp as *mut c_void) };
        }
    }
    unsafe { libc::free(replies as *mut c_void) };
}

/// Pure decision function: what final PAM return code should this attempt report, given
/// `pam_authenticate`'s result and (if authentication succeeded) `pam_acct_mgmt`'s result.
///
/// An unprivileged locker can't read `/etc/shadow` for the account stack, so
/// `PAM_AUTHINFO_UNAVAIL` from `pam_acct_mgmt` is treated as success: `pam_authenticate`
/// already proved identity. Kept as a standalone function (rather than inline in `run`) so
/// it is unit-testable without a live PAM stack.
fn resolve_final_rc(auth_rc: c_int, acct_rc: Option<c_int>) -> c_int {
    if auth_rc != PAM_SUCCESS {
        return auth_rc;
    }
    match acct_rc {
        Some(rc) if rc != PAM_SUCCESS && rc != PAM_AUTHINFO_UNAVAIL => rc,
        _ => PAM_SUCCESS,
    }
}

fn pam_error_text(pamh: *mut pam_sys::pam_handle_t, code: c_int) -> String {
    let ptr = unsafe { pam_strerror(pamh, code) };
    if ptr.is_null() {
        return format!("PAM error {code}");
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Runs the PAM conversation for `service` and never returns: it always exits the process
/// (0 on success, 1 on failure) after emitting the terminal `SUCCESS`/`FAILURE` line.
pub fn run(service: &str) -> ! {
    let Some(username) = resolve_current_username() else {
        emit_line("FAILURE could not resolve current username");
        std::process::exit(1);
    };

    let Ok(c_service) = CString::new(service) else {
        emit_line("FAILURE invalid service name");
        std::process::exit(1);
    };
    let Ok(c_user) = CString::new(username) else {
        emit_line("FAILURE invalid username");
        std::process::exit(1);
    };

    let conv = pam_conv {
        conv: Some(conversation_callback),
        appdata_ptr: ptr::null_mut(),
    };

    let mut pamh: *mut pam_sys::pam_handle_t = ptr::null_mut();
    let start_rc = unsafe { pam_start(c_service.as_ptr(), c_user.as_ptr(), &conv, &mut pamh) };
    if start_rc != PAM_SUCCESS || pamh.is_null() {
        emit_line("FAILURE pam_start failed");
        std::process::exit(1);
    }

    let auth_rc = unsafe { pam_authenticate(pamh, 0) };
    let acct_rc = if auth_rc == PAM_SUCCESS {
        Some(unsafe { pam_sys::pam_acct_mgmt(pamh, 0) })
    } else {
        None
    };
    let final_rc = resolve_final_rc(auth_rc, acct_rc);

    let success = final_rc == PAM_SUCCESS;
    if success {
        emit_line("SUCCESS");
    } else {
        let message = pam_error_text(pamh, final_rc);
        emit_line(&format!("FAILURE {message}"));
    }

    unsafe {
        pam_end(pamh, final_rc);
    }

    std::process::exit(if success { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pam_sys::{PAM_CRED_INSUFFICIENT, PAM_USER_UNKNOWN};

    #[test]
    fn auth_failure_is_reported_regardless_of_acct_mgmt() {
        assert_eq!(resolve_final_rc(PAM_USER_UNKNOWN, None), PAM_USER_UNKNOWN);
    }

    #[test]
    fn auth_success_with_acct_success_is_success() {
        assert_eq!(
            resolve_final_rc(PAM_SUCCESS, Some(PAM_SUCCESS)),
            PAM_SUCCESS
        );
    }

    #[test]
    fn auth_success_with_acct_authinfo_unavail_is_still_success() {
        // The load-bearing case: an unprivileged locker can't read /etc/shadow for the
        // account stack, so this must not be reported as a failure.
        assert_eq!(
            resolve_final_rc(PAM_SUCCESS, Some(PAM_AUTHINFO_UNAVAIL)),
            PAM_SUCCESS
        );
    }

    #[test]
    fn auth_success_with_other_acct_failure_is_failure() {
        assert_eq!(
            resolve_final_rc(PAM_SUCCESS, Some(PAM_CRED_INSUFFICIENT)),
            PAM_CRED_INSUFFICIENT
        );
    }
}

//! Extension lifecycle trait and export macro.

use crate::bindings::shilpo::extension::events::ExtensionEvent;
use crate::bindings::shilpo::extension::types::{
    Activation, DeactivateReason, Error, SearchCandidate, SearchRequest,
};
use crate::bindings::shilpo::extension::view::ViewTree;

/// High-level extension lifecycle trait.
///
/// Extension authors implement this trait and register their implementation
/// with [`export_extension!`].
pub trait Extension: Default {
    /// Called when the extension component is activated by the host.
    fn activate(&mut self, activation: Activation) -> Result<(), Error> {
        let _ = activation;
        Ok(())
    }

    /// Called when the extension component is being deactivated.
    fn deactivate(&mut self, reason: DeactivateReason) -> Result<(), Error> {
        let _ = reason;
        Ok(())
    }

    /// Called when an inbound event occurs (lifecycle, user input, state change, etc.).
    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        let _ = event;
        Ok(())
    }

    /// Called by the host to render a declarative ViewTree for a UI contribution.
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        let _ = contribution_id;
        Ok(None)
    }

    /// Called by the host to query an extension search provider.
    fn search(
        &mut self,
        contribution_id: &str,
        request: SearchRequest,
    ) -> Result<Vec<SearchCandidate>, Error> {
        let _ = (contribution_id, request);
        Ok(Vec::new())
    }
}

/// Runs an extension callback without allowing a guest panic to unwind through
/// the component boundary. The panic payload is intentionally not exposed to
/// the host because it may contain extension secrets or other sensitive data.
pub fn invoke_callback<T>(callback: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)) {
        Ok(result) => result,
        Err(_) => Err(Error {
            kind: crate::bindings::shilpo::extension::types::ErrorKind::Internal,
            message: "extension callback panicked".into(),
        }),
    }
}

/// Exports an [`Extension`] implementation to the canonical WebAssembly guest boundary.
///
/// # Examples
///
/// ```rust,no_run
/// use shilpo_ext_sdk::prelude::*;
///
/// #[derive(Default)]
/// struct MyExtension;
///
/// impl Extension for MyExtension {
///     fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
///         if contribution_id == "widget" {
///             Ok(Some(view! {
///                 row() {
///                     text("Hello World!"),
///                 }
///             }))
///         } else {
///             Ok(None)
///         }
///     }
/// }
///
/// export_extension!(MyExtension);
/// ```
#[macro_export]
macro_rules! export_extension {
    ($ext:ty) => {
        #[allow(unused_imports)]
        use $crate::bindings::export;

        struct __ShilpoGuestComponent;

        static mut __SHILPO_INSTANCE: ::core::option::Option<$ext> = ::core::option::Option::None;

        fn __with_instance<T>(
            callback: impl FnOnce(&mut $ext) -> ::core::result::Result<T, $crate::bindings::shilpo::extension::types::Error>,
        ) -> ::core::result::Result<T, $crate::bindings::shilpo::extension::types::Error> {
            unsafe {
                if __SHILPO_INSTANCE.is_none() {
                    __SHILPO_INSTANCE = ::core::option::Option::Some(
                        <$ext as ::core::default::Default>::default(),
                    );
                }
                match __SHILPO_INSTANCE.as_mut() {
                    ::core::option::Option::Some(instance) => callback(instance),
                    ::core::option::Option::None => ::core::result::Result::Err(
                        $crate::bindings::shilpo::extension::types::Error {
                            kind: $crate::bindings::shilpo::extension::types::ErrorKind::Internal,
                            message: "extension instance unavailable".into(),
                        },
                    ),
                }
            }
        }

        impl $crate::bindings::Guest for __ShilpoGuestComponent {
            fn activate(
                activation: $crate::bindings::shilpo::extension::types::Activation,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                $crate::extension::invoke_callback(|| {
                    __with_instance(|inst| <$ext as $crate::extension::Extension>::activate(inst, activation))
                })
            }

            fn deactivate(
                reason: $crate::bindings::shilpo::extension::types::DeactivateReason,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                let res = $crate::extension::invoke_callback(|| {
                    __with_instance(|inst| <$ext as $crate::extension::Extension>::deactivate(inst, reason))
                });
                unsafe {
                    __SHILPO_INSTANCE = ::core::option::Option::None;
                }
                res
            }

            fn on_event(
                event: $crate::bindings::shilpo::extension::events::ExtensionEvent,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                $crate::extension::invoke_callback(|| {
                    __with_instance(|inst| <$ext as $crate::extension::Extension>::on_event(inst, event))
                })
            }

            fn view(
                contribution_id: ::std::string::String,
            ) -> ::core::result::Result<
                ::core::option::Option<$crate::bindings::shilpo::extension::view::ViewTree>,
                $crate::bindings::shilpo::extension::types::Error,
            > {
                $crate::extension::invoke_callback(|| {
                    __with_instance(|inst| <$ext as $crate::extension::Extension>::view(inst, &contribution_id))
                })
            }

            fn search(
                contribution_id: ::std::string::String,
                request: $crate::bindings::shilpo::extension::types::SearchRequest,
            ) -> ::core::result::Result<
                ::std::vec::Vec<$crate::bindings::shilpo::extension::types::SearchCandidate>,
                $crate::bindings::shilpo::extension::types::Error,
            > {
                $crate::extension::invoke_callback(|| {
                    __with_instance(|inst| <$ext as $crate::extension::Extension>::search(inst, &contribution_id, request))
                })
            }
        }

        $crate::bindings::export!(__ShilpoGuestComponent with_types_in $crate::bindings);
    };
}

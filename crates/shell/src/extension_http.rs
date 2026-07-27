use shilpo_ext::ExtensionEvent;

pub(crate) async fn fetch(request_id: String, url: String, method: String) -> ExtensionEvent {
    const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

    let fail = |error: String| ExtensionEvent::HttpResponse {
        request_id: request_id.clone(),
        status: None,
        body: String::new(),
        error: Some(error),
    };
    if method != "GET" {
        return fail("only GET requests are supported".into());
    }
    if !url.starts_with("https://") {
        return fail("extension HTTP requests require HTTPS".into());
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect_policy(reqwest::redirect::Policy::none())
        .user_agent("Shilpo Extension Host/0.1")
        .build()
    {
        Ok(client) => client,
        Err(error) => return fail(format!("failed to initialize HTTP transport: {error}")),
    };
    let mut response = match client.get(&url).send().await {
        Ok(response) => response,
        Err(error) => return fail(format!("request failed: {error}")),
    };
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return fail("response exceeds the 1 MiB limit".into());
    }
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return fail("response exceeds the 1 MiB limit".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(error) => return fail(format!("failed to read response: {error}")),
        }
    }
    match String::from_utf8(bytes) {
        Ok(body) => ExtensionEvent::HttpResponse {
            request_id,
            status: Some(status),
            body,
            error: None,
        },
        Err(_) => fail("response is not valid UTF-8".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::fetch;
    use shilpo_ext::ExtensionEvent;

    #[tokio::test]
    async fn rejects_unsafe_requests_before_network_io() {
        let response = fetch("request".into(), "http://example.com".into(), "GET".into()).await;
        assert!(matches!(
            response,
            ExtensionEvent::HttpResponse {
                status: None,
                error: Some(error),
                ..
            } if error.contains("HTTPS")
        ));

        let response = fetch(
            "request".into(),
            "https://example.com".into(),
            "POST".into(),
        )
        .await;
        assert!(matches!(
            response,
            ExtensionEvent::HttpResponse {
                status: None,
                error: Some(error),
                ..
            } if error.contains("only GET")
        ));
    }
}

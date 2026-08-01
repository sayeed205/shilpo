use shilpo_ext::{AuthorizedHttpRequest, ExtensionEvent};

pub(crate) fn build_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect_policy(reqwest::redirect::Policy::none())
        .user_agent("Shilpo Extension Host/0.1")
        .build()
}

pub(crate) fn build_request(
    client: &reqwest::Client,
    request: &AuthorizedHttpRequest,
) -> reqwest::Request {
    client
        .get(request.url().clone())
        .build()
        .expect("valid request")
}

pub(crate) async fn fetch(request: AuthorizedHttpRequest) -> ExtensionEvent {
    const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
    let request_id = request.request_id().to_string();

    let fail = |error: String| ExtensionEvent::HttpResponse {
        request_id: request_id.clone(),
        status: None,
        body: String::new(),
        error: Some(error),
    };

    let client = match build_client() {
        Ok(client) => client,
        Err(error) => return fail(format!("failed to initialize HTTP transport: {error}")),
    };
    let req = build_request(&client, &request);
    let mut response = match client.execute(req).await {
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
    use super::*;
    use shilpo_ext::{
        AuthorizedHostEffectKind, Capability, ExtensionHost, ExtensionManifest, GuestExtension,
        HostEffect, InMemoryRuntime,
    };

    struct TestGuest(HostEffect);
    impl GuestExtension for TestGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostEffect> {
            vec![self.0.clone()]
        }

        fn view(&self, _: &str) -> Option<shilpo_ext::ViewTree> {
            None
        }
    }

    #[tokio::test]
    async fn builds_get_from_the_exact_authorized_url() {
        let manifest = ExtensionManifest::from_toml(
            r#"
            id = "io.github.test.http"
            name = "HTTP Test"
            version = "1.0.0"

            [[capabilities]]
            kind = "network:http"
            hosts = ["api.example.com"]
            paths = ["/clock/*"]
            "#,
        )
        .unwrap();

        let mut host = ExtensionHost::<InMemoryRuntime>::default();
        host.register(
            manifest.clone(),
            Box::new(TestGuest(HostEffect::HttpRequest {
                request_id: "test1".into(),
                url: "https://api.example.com/clock/current".into(),
                method: "GET".into(),
            })),
            vec![Capability::NetworkHttp {
                hosts: vec!["api.example.com".into()],
                paths: vec!["/clock/*".into()],
            }],
        )
        .unwrap();

        let result = host
            .dispatch_event(&manifest.id, &ExtensionEvent::ShellStarted)
            .unwrap();
        assert_eq!(result.accepted.len(), 1);

        let AuthorizedHostEffectKind::HttpRequest(auth_req) = result.accepted[0].kind() else {
            panic!("expected AuthorizedHttpRequest");
        };

        let client = build_client().unwrap();
        let built_req = build_request(&client, auth_req);

        assert_eq!(built_req.method(), reqwest::Method::GET);
        assert_eq!(built_req.url(), auth_req.url());
    }
}

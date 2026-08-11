use shilpo_ext_api::ExtensionEvent;
use shilpo_ext_runtime::AuthorizedHttpRequest;

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
        .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
        .build()
        .expect("authorized HTTP request builds valid reqwest::Request")
}

pub(crate) async fn fetch(request: AuthorizedHttpRequest) -> ExtensionEvent {
    let request_id = request.request_id().to_string();
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            return ExtensionEvent::HttpResponse {
                request_id,
                status: None,
                body: String::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let req = build_request(&client, &request);
    let res = client.execute(req).await;
    build_response_event(request_id, res).await
}

pub(crate) async fn build_response_event(
    request_id: String,
    result: Result<reqwest::Response, reqwest::Error>,
) -> ExtensionEvent {
    match result {
        Ok(response) => {
            let status = Some(response.status().as_u16());
            match response.text().await {
                Ok(body) => ExtensionEvent::HttpResponse {
                    request_id,
                    status,
                    body,
                    error: None,
                },
                Err(error) => ExtensionEvent::HttpResponse {
                    request_id,
                    status,
                    body: String::new(),
                    error: Some(error.to_string()),
                },
            }
        }
        Err(error) => ExtensionEvent::HttpResponse {
            request_id,
            status: None,
            body: String::new(),
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext_api::{Capability, ExtensionManifest, HostEffect, ViewTree};
    use shilpo_ext_runtime::{
        AuthorizedHostEffectKind, ExtensionHost, GuestExtension, InMemoryRuntime,
    };

    struct TestGuest(HostEffect);
    impl GuestExtension for TestGuest {
        fn on_event(&mut self, _: &ExtensionEvent) -> Vec<HostEffect> {
            vec![self.0.clone()]
        }

        fn view(&self, _: &str) -> Option<ViewTree> {
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

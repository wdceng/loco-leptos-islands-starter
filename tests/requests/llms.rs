//! `/llms.txt` over HTTP: Markdown with a title and links, and every link
//! leads somewhere.

use app::{app::App, views::layout::APP_NAME};
use loco_rs::testing::prelude::*;
use regex::Regex;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn llms_txt_names_the_site_and_its_pages() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/llms.txt").await;
        assert_eq!(response.status_code(), 200);
        assert!(
            response
                .header("content-type")
                .to_str()
                .unwrap_or("")
                .starts_with("text/markdown"),
            "served as Markdown"
        );
        let body = response.text();
        assert!(
            body.starts_with(&format!("# {APP_NAME}\n")),
            "one H1 first:\n{body}"
        );

        // Links start with `server.host` (config/test.yaml), and each one
        // answers.
        let link = Regex::new(r"\]\(http://localhost:5150(/[^)]*)\)").expect("regex");
        let paths: Vec<String> = link
            .captures_iter(&body)
            .map(|c| c[1].to_string())
            .collect();
        assert!(
            !paths.is_empty(),
            "no links built from server.host:\n{body}"
        );
        for path in paths {
            let status = request
                .get(&path)
                .add_header("accept", "text/html")
                .await
                .status_code();
            assert_eq!(status, 200, "{path} from llms.txt answers {status}");
        }
    })
    .await;
}

use app::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// The test harness boots with `config/test.yaml`, so this exercises the
/// "not production" branch over HTTP. Both branches of the body itself are
/// unit-tested next to the controller.
#[tokio::test]
#[serial]
async fn robots_is_closed_outside_production() {
    request::<App, _, _>(|request, _ctx| async move {
        let res = request.get("/robots.txt").await;

        assert_eq!(res.status_code(), 200);
        let content_type = res.header("content-type");
        assert!(
            content_type
                .to_str()
                .unwrap_or("")
                .starts_with("text/plain"),
            "unexpected content type: {content_type:?}"
        );
        assert_eq!(res.text(), "User-agent: *\nDisallow: /\n");
    })
    .await;
}

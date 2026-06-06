#[tokio::test]
async fn dashboard_launcher_starts_background_server_without_browser() {
    let temp_dir = tempfile::tempdir().expect("temp JULIE_HOME should be created");
    let paths = crate::paths::DaemonPaths::with_home(temp_dir.path().join(".julie"));

    let launch = crate::dashboard::standalone::launch_dashboard_for_paths(
        paths,
        crate::dashboard::standalone::DashboardLaunchOptions {
            open_browser: false,
        },
    )
    .await
    .expect("dashboard launcher should bind and spawn a background server");

    assert!(launch.url.starts_with("http://127.0.0.1:"));
    tokio::net::TcpStream::connect(launch.local_addr)
        .await
        .expect("background dashboard server should accept local connections");
}

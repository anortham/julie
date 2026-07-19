/// Sends a request when the feature is enabled.
pub async fn fetch_user(enabled: bool, retries: usize) -> Result<(), reqwest::Error> {
    if enabled {
        for _ in 0..retries {
            reqwest::Client::new()
                .get("/api/users/{id}")
                .send()
                .await?;
        }
    }
    Ok(())
}

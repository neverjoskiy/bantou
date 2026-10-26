//! Сетевой слой: загрузка страниц через reqwest (rustls, без системных зависимостей).

use std::time::Duration;

/// Загружает URL и возвращает тело как строку (потеря некорректных UTF-8 — replacement).
pub async fn fetch_text(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    let bytes = resp.bytes().await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn build_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("Bantou/{}", env!("CARGO_PKG_VERSION")))
        .build()
}

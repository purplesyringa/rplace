use anyhow::{bail, Result};

const AUTH_LINKS: &'static [u32] = &[0, 31027, 32030, 33030, 34030, 35025];

async fn check_contest(login: String, password: String, contest_id: u32) -> Result<bool> {
    let res = reqwest::Client::new()
        .post("https://ejudge.algocode.ru/cgi-bin/new-client")
        .form(&[
            ("contest_id", contest_id.to_string()),
            ("role", "0".to_string()),
            ("prob_name", "".to_string()),
            ("login", login),
            ("password", password),
            ("locale_id", "1".to_string()),
            ("action_2", "Войти".to_string()),
        ])
        .send()
        .await?;
    Ok(!res.text().await?.contains("SID=\"0000000000000000\""))
}

pub async fn check_account(login: &str, password: &str, group: usize) -> Result<bool> {
    if group < 1 || group >= AUTH_LINKS.len() {
        bail!("Invalid group");
    }

    if check_contest(login.to_string(), password.to_string(), AUTH_LINKS[group]).await? {
        return Ok(true);
    }

    for contest_id in &AUTH_LINKS[1..] {
        if *contest_id != AUTH_LINKS[group] {
            if check_contest(login.to_string(), password.to_string(), *contest_id).await? {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

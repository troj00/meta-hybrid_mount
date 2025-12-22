use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::thread;
use std::time::Duration;

fn main() {
    let bot_token = env::var("TELEGRAM_BOT_TOKEN").expect("Error: TELEGRAM_BOT_TOKEN not set");
    let chat_id = env::var("TELEGRAM_CHAT_ID").expect("Error: TELEGRAM_CHAT_ID not set");

    let args: Vec<String> = env::args().collect();
    let topic_id = if args.len() > 1 { Some(&args[1]) } else { None };
    let event_label = if args.len() > 2 {
        &args[2]
    } else {
        "New Yield (新产物)"
    };

    let repo = env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let run_id = env::var("GITHUB_RUN_ID").unwrap_or_default();
    let server_url = env::var("GITHUB_SERVER_URL").unwrap_or("https://github.com".to_string());
    let run_url = format!("{}/{}/actions/runs/{}", server_url, repo, run_id);

    let output_dir = PathBuf::from("output");
    let mut zip_file: Option<PathBuf> = None;

    if let Ok(entries) = fs::read_dir(&output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "zip" {
                    if let Ok(abs_path) = fs::canonicalize(&path) {
                        zip_file = Some(abs_path);
                    } else {
                        zip_file = Some(path);
                    }
                    break;
                }
            }
        }
    }

    let file_path = match zip_file {
        Some(p) => p,
        None => {
            eprintln!("Error: No grain sacks (zip files) found in output/.");
            exit(1);
        }
    };

    let file_name = file_path.file_name().unwrap().to_string_lossy();
    let file_size = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0) as f64 / 1024.0 / 1024.0;

    println!("Selecting yield: {} ({:.2} MB)", file_name, file_size);
    println!("Debug: Absolute path is {}", file_path.display());

    let commit_msg = get_git_commit_message();
    let safe_commit_msg = escape_html(&commit_msg);

    let caption = format!(
        "🌾 <b>Meta-Hybrid: {}</b>\n\n\
        ⚖️ <b>重量 (Weight):</b> {:.2} MB\n\n\
        📝 <b>新性状 (Commit):</b>\n\
        <pre>{}</pre>\n\n\
        🚜 <a href='{}'>查看日志 (View Log)</a>",
        event_label, file_size, safe_commit_msg, run_url
    );

    let url = format!("https://api.telegram.org/bot{}/sendDocument", bot_token);
    let mut curl_args = vec![
        "-F".to_string(),
        format!("chat_id={}", chat_id),
        "-F".to_string(),
        format!("document=@{}", file_path.display()),
        "-F".to_string(),
        format!("caption={}", caption),
        "-F".to_string(),
        "parse_mode=HTML".to_string(),
        url.clone(),
    ];

    if let Some(tid) = topic_id {
        if !tid.trim().is_empty() && tid != "0" {
            curl_args.insert(0, format!("message_thread_id={}", tid));
            curl_args.insert(0, "-F".to_string());
            println!("Targeting Topic ID: {}", tid);
        }
    }

    println!("Dispatching yield to Granary (Telegram)...");

    let max_retries = 2;
    for attempt in 0..max_retries {
        let (success, response) = run_curl(&curl_args);

        if success && response.contains("\"ok\":true") {
            println!("✅ Yield stored successfully!");
            return;
        }

        if response.contains("\"ok\":false") {
            println!("⚠️ Telegram API rejected: {}", response);
        }

        if response.contains("\"error_code\":400") && response.contains("TOPIC_CLOSED") {
            if attempt < max_retries - 1 {
                if let Some(tid) = topic_id {
                    if reopen_topic(&bot_token, &chat_id, tid) {
                        println!("🔄 Retrying upload in 2 seconds...");
                        thread::sleep(Duration::from_secs(2));
                        continue;
                    } else {
                        eprintln!("❌ Could not reopen topic. Aborting.");
                        exit(1);
                    }
                }
            } else {
                eprintln!("❌ Retries exhausted.");
            }
        }

        eprintln!(
            "❌ Storage failed (Attempt {}/{}): {}",
            attempt + 1,
            max_retries,
            response
        );
        if attempt == max_retries - 1 {
            exit(1);
        }
        thread::sleep(Duration::from_secs(2));
    }
}

fn get_git_commit_message() -> String {
    let output = Command::new("git")
        .args(["log", "-1", "--pretty=%B"])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "No commit message available.".to_string(),
    }
}

fn run_curl(args: &[String]) -> (bool, String) {
    match Command::new("curl").args(["-s", "-S"]).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                (true, stdout)
            } else {
                let err_msg = if !stderr.is_empty() { stderr } else { stdout };
                (false, err_msg)
            }
        }
        Err(e) => (false, e.to_string()),
    }
}

fn reopen_topic(bot_token: &str, chat_id: &str, topic_id: &str) -> bool {
    let url = format!("https://api.telegram.org/bot{}/reopenForumTopic", bot_token);
    let data = format!(
        r#"{{"chat_id": "{}", "message_thread_id": {}}}"#,
        chat_id, topic_id
    );

    println!("⚠️ Topic {} is closed. Attempting to reopen...", topic_id);

    let args = vec![
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        "-d".to_string(),
        data,
        "-X".to_string(),
        "POST".to_string(),
        url,
    ];

    let (success, response) = run_curl(&args);

    if success && response.contains("\"ok\":true") {
        println!("✅ Topic {} successfully reopened!", topic_id);
        true
    } else {
        eprintln!("❌ Failed to reopen topic: {}", response);
        false
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

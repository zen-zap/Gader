use bollard::{API_DEFAULT_VERSION, Docker, query_parameters::LogsOptionsBuilder};
use futures::{StreamExt};
use regex::Regex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker_connection =
        Docker::connect_with_http("http://127.0.0.1:2375", 5, API_DEFAULT_VERSION)
            .expect("Unable to connect to docker");

    // services to watch out for
    // immich and vaultwarden
    let immich: String = String::from("immich_server");
    let _vw: String = String::from("vaultwarden");

    let params = LogsOptionsBuilder::new()
        .follow(true)
        .stderr(true)
        .stdout(true)
        .tail("7")
        .build();

    let mut immich_logs = docker_connection.logs(immich.as_str(), Some(params));

    let log_pattern = r"\[Nest\]\s+\d+\s+-\s+(?P<time>\d{2}/\d{2}/\d{4},\s+\d{1,2}:\d{2}:\d{2}\s+[AP]M)\s+(?P<level>[A-Z]+)\s+\[(?P<context>[^\]]+)\]\s+(?P<msg>.+)";

    let ansi_re = Regex::new(r"\x1b\[[0-9;]*m")?;

    let re = Regex::new(log_pattern)?;

    // we need to stream this
    while let Some(res) = immich_logs.next().await {
        match res {
            Ok(log_output) => {
                println!("{}", log_output);

                let lg_output = log_output.to_string();
                let stripped_ansi = ansi_re.replace_all(&lg_output, "").to_string();
                
                if let Some(caps) = re.captures(&stripped_ansi.as_str()) {
                    println!("Time: {}", &caps["time"]);
                    println!("Level: {}", &caps["level"]);
                    println!("Context: {}", &caps["context"]);
                    println!("Message: {} \n\n", &caps["msg"]);
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    Ok(())
}

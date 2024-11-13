use std::{
    fs,
    io::{self, Write},
    process,
};

use clap::Parser;
use config::{self, Config, ConfigError, File, FileFormat};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Parser, Debug)]
#[command(name = "gitlab roulette")]
struct Cli {
    #[arg(short, long, help = "URL of the project")]
    url: Option<String>,

    #[arg(short, long, help = "Gitlab token to use to connect")]
    token: Option<String>,

    #[arg(
        short,
        long,
        help = "File to use as config",
        default_value = "./gitlab-roulette.toml"
    )]
    config_file: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitlabProject {
    id: i32,
    name: String,
    path_with_namespace: String,
    web_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitlabIssue {
    id: i32,
    project_id: i32,
    title: String,
    description: String,
    state: String,
    r#type: String,
    assignees: Vec<GitlabIssueAssigne>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitlabIssueAssigne {
    id: i32,
    state: String,
    username: String,
    name: String,
}

fn main() -> Result<(), ConfigError> {
    let cli = Cli::parse();

    let config_file = cli.config_file.unwrap();

    let mut builder = Config::builder();
    if fs::exists(&config_file).expect("failed to check for config file") {
        builder = builder.add_source(File::new(&config_file, FileFormat::Toml));
    }
    //  .add_async_source(...)
    builder = builder
        .set_override_option("url", cli.url)?
        .set_override_option("token", cli.token)?;

    let config = builder.build()?;

    let url = config.get_string("url");
    let token = config.get_string("token");

    if !url.is_ok() {
        eprintln!("Please add a url to the config file or using the --url argument");
        process::exit(1);
    }

    let url = url.unwrap();
    let url_parse = Url::parse(&url);
    if !url_parse.is_ok() {
        eprintln!("the url \"{}\" is not valid", url);
        process::exit(1);
    }
    let url_parse = url_parse.unwrap();

    let gitlab_domain = format!(
        "{}://{}",
        url_parse.scheme().to_string(),
        url_parse
            .domain()
            .expect("failed to extract the domain out of the url")
    );

    if !token.is_ok() {
        eprintln!("Please add a token to the config file or using the --token argument");
        process::exit(1);
    }

    let token = token.unwrap();

    let client = reqwest::blocking::Client::new();
    let res = client
        .get(format!(
            "{}/api/v4/projects?membership=true&simple=true",
            gitlab_domain
        ))
        .header("PRIVATE-TOKEN", token.clone())
        .send();

    if res.is_err() {
        eprintln!("failed to send request");
        process::exit(1);
    }

    let res = res.unwrap();
    let res = res.text().expect("failed to get response body");
    let projects = serde_json::from_str::<Vec<GitlabProject>>(&res);
    let projects = projects.expect("failed to parse json");

    // try to find the project using URL
    let project = projects.iter().find(|p| p.web_url == url);
    let project = if let Some(project) = project {
        println!("Found project: {}", project.name);
        project
    } else {
        loop {
            println!("Couldn't find the project using the url, select one below:");
            for (i, e) in projects.iter().enumerate() {
                println!("{}. {}", i + 1, e.path_with_namespace);
            }
            print!("> ");
            io::stdout().flush().expect("failed to flush output");
            let mut input: String = "".to_string();
            io::stdin()
                .read_line(&mut input)
                .expect("unable to read user data");

            match input.trim().parse::<i32>() {
                Ok(num) => {
                    if num > 0 && num <= projects.len() as i32 {
                        let num = num as usize;
                        break projects.get(num - 1).unwrap();
                    }
                }
                Err(_) => {}
            }
            println!();
        }
    };

    let res = client
        .get(format!(
            "{}/api/v4/projects/{}/issues",
            gitlab_domain, project.id
        ))
        .header("PRIVATE-TOKEN", token.clone())
        .send();

    if res.is_err() {
        eprintln!("failed to send request");
        process::exit(1);
    }

    let res = res.unwrap();
    let res = res.text().expect("failed to get response body");
    let issues = serde_json::from_str::<Vec<GitlabIssue>>(&res);

    println!("{:?}", issues);
    return Ok(());
}

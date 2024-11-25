use ansi_escapes::{self, CursorBackward, CursorDown, CursorLeft, CursorPrevLine, CursorUp};
use clap::Parser;
use config::{self, Config, ConfigError, File, FileFormat};
use dialoguer::Confirm;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};
use rand::seq::SliceRandom;
use rand::{self, Rng};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::io::{stdout, Write};
use std::thread;
use std::time::Duration;
use std::{fmt::Display, fs, process};
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
    iid: i32,
    project_id: i32,
    title: String,
    description: String,
    state: String,
    r#type: String,
    assignees: Vec<GitlabProjectMember>,
    milestone: Option<GitlabMilestone>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitlabMilestone {
    id: i32,
    project_id: i32,
    title: String,
    description: String,
    state: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitlabProjectMember {
    id: i32,
    username: String,
    name: String,
}

#[derive(Debug)]
enum IssueSelectionType {
    Milestone,
    Range,
    Manual,
}

impl Display for IssueSelectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Display for GitlabIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}: {}", self.iid, self.title)
    }
}

impl Display for GitlabProjectMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.username)
    }
}

impl Display for GitlabMilestone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}: {}", self.id, self.title)
    }
}

impl PartialEq for GitlabMilestone {
    fn eq(&self, other: &Self) -> bool {
        return self.id == other.id;
    }
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
        let projects_names: Vec<String> = projects
            .iter()
            .map(|proj| proj.path_with_namespace.clone())
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a project: ")
            .items(&projects_names)
            .interact()
            .unwrap();

        &projects[selection]
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
    let issues = serde_json::from_str::<Vec<GitlabIssue>>(&res).expect("failed to parse issues");

    let res = client
        .get(format!(
            "{}/api/v4/projects/{}/members",
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
    let members: Vec<GitlabProjectMember> =
        serde_json::from_str(&res).expect("failed to parse members");

    let selection_types = vec![
        IssueSelectionType::Milestone,
        IssueSelectionType::Range,
        IssueSelectionType::Manual,
    ];

    let selection_type_res = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select the way you want to select the issues:")
        .items(&selection_types)
        .interact()
        .unwrap();

    let selection_type = &selection_types[selection_type_res];

    let selected_issues: Vec<&GitlabIssue> = match selection_type {
        IssueSelectionType::Manual => {
            let selection = MultiSelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select all the issues that you want to use: ")
                .items(&issues)
                .interact()
                .unwrap();

            let selected_issues: Vec<&GitlabIssue> =
                selection.into_iter().map(|i| &issues[i]).collect();

            selected_issues
        }
        IssueSelectionType::Milestone => {
            let mut milestones: Vec<&GitlabMilestone> = Vec::new();
            issues.iter().for_each(|issue| {
                if let Some(milestone) = &issue.milestone {
                    if !milestones.contains(&milestone) {
                        milestones.push(milestone);
                    }
                }
            });

            let selection = MultiSelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select all the milestones that you want to use: ")
                .items(&milestones)
                .interact()
                .unwrap();

            let selected_milestones: Vec<&GitlabMilestone> =
                selection.into_iter().map(|i| milestones[i]).collect();

            let selected_issues: Vec<&GitlabIssue> = issues
                .iter()
                .filter(|issue| {
                    issue.milestone.is_some()
                        && selected_milestones.contains(&issue.milestone.as_ref().unwrap())
                })
                .collect();

            selected_issues
        }
        IssueSelectionType::Range => {
            let range_start = issue_id_select(&issues, "Enter the ID of the first issue:");
            let range_end = issue_id_select(&issues, "Enter the ID of the last issue:");

            let selected_issues: Vec<&GitlabIssue> = issues
                .iter()
                .filter(|issue| issue.id >= range_start && issue.id <= range_end)
                .collect();
            selected_issues
        }
    };

    let selected_members = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select all the members you want to asign the issues to:")
        .items(&members)
        .interact()
        .unwrap();
    let selected_members: Vec<&GitlabProjectMember> =
        selected_members.into_iter().map(|i| &members[i]).collect();

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Do you want to continue ?")
        .default(true)
        .interact()
        .unwrap();

    if !confirm {
        process::exit(0);
    }
    let mut rng = rand::thread_rng();
    // selected_issues.shuffle(&mut rng);
    let issue_per_member = selected_issues.len() / selected_members.len();
    let rest = selected_issues.len() % selected_members.len();
    let mut assignements: Vec<usize> = (0..selected_members.len())
        .flat_map(|i| (0..issue_per_member).map(move |_| i))
        .collect();
    for _ in 0..rest {
        assignements.push(rng.gen_range(0..selected_members.len()));
    }
    assignements.shuffle(&mut rng);

    println!("");
    for (i, issue) in selected_issues.iter().enumerate() {
        let rand_member = selected_members[assignements[i]];
        // animation
        println!("{}", issue);
        println!("\t{}", rand_member);
    }

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Do you want to confirm this assignment ?")
        .interact()
        .unwrap();
    if !confirm {
        println!("Exiting");
        process::exit(0);
    }

    for (i, issue) in selected_issues.iter().enumerate() {
        let rand_member = selected_members[assignements[i]];
        let res = client
            .put(format!(
                "{}/api/v4/projects/{}/issues/{}?assignee_ids={}",
                gitlab_domain, project.id, issue.iid, rand_member.id
            ))
            .header("PRIVATE-TOKEN", token.clone())
            .send();

        if res.is_err() {
            eprintln!("failed to send request");
            process::exit(1);
        }

        let res = res.unwrap();
        if res.status() != StatusCode::OK {
            eprintln!("failed to assign issue {}", issue);
            process::exit(1);
        }
    }

    println!("issues assigned !");

    return Ok(());
}

fn issue_id_select(issues: &Vec<GitlabIssue>, prompt: &str) -> i32 {
    let issue_id = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .validate_with(|input: &String| {
            let num = input.parse::<i32>();
            match num {
                Ok(num) => {
                    let issue = issues.iter().find(|issue| issue.id == num);
                    match issue {
                        Some(_) => Ok(()),
                        None => Err("Issue cannot be found"),
                    }
                }
                Err(_) => Err("Input is not a number"),
            }
        })
        .interact()
        .unwrap()
        .parse::<i32>()
        .unwrap();

    return issue_id;
}

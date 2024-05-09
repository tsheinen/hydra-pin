use std::fmt::{format, write, Display};
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{collections::HashMap, path::PathBuf};

use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;

use clap::{Parser, Subcommand};
use regex::Regex;
use reqwest::blocking::{get, Client};
use serde::Deserialize;
use serde_json::Value;

#[derive(Subcommand, Debug)]
enum Action {
    Pin,
    Unpin,
}

#[derive(Parser, Debug)]
struct Args {
    /// hydra-check binary to use
    #[arg(short = 'b', long, env)]
    hydra_check: Option<String>,
    ///
    #[arg(short, long)]
    package: String,
    /// Output Nix file to write
    #[arg(short, long)]
    output: PathBuf,
    #[command(subcommand)]
    command: Action,
}

#[derive(Deserialize)]
struct Job {
    success: bool,
    build_id: String,
}

#[derive(Deserialize)]
struct Build {
    jobsetevals: Vec<u64>,
}

#[derive(Deserialize, Debug)]
struct Input {
    uri: Option<String>,
    r#type: Option<String>,
    revision: Option<String>,
}

#[derive(Deserialize)]
struct Eval {
    jobsetevalinputs: HashMap<String, Input>,
}

struct Package {
    name: String,
    url: String,
    sha256: String,
}

impl Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
                    "{} = (import (fetchTarball {{
            url = \"{}\";
            sha256 = \"{}\";
        }}) {{ system = pkgs.system; }}).{};
        ",
            &self.name, &self.url, &self.sha256, &self.name
        )?;
        Ok(())
    }
}

struct Overlay {
    packages: Vec<Package>,
}

impl Display for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = self
            .packages
            .iter()
            .map(|pkg| format!("# {} {} {}\n", pkg.name, pkg.url, pkg.sha256))
            .collect::<String>();
        write!(
            f,
            "{}
{{pkgs}}: {{
    overlay = (final: prev: {{
",
            prefix
        )?;

        for pkgs in &self.packages {
            write!(f, "{}\n", pkgs)?;
        }
        write!(
            f,
            "        
    }});
}}"
        )?;
        Ok(())
    }
}

fn get_package(args: &Args) -> Result<Package> {
    let stdout = std::process::Command::new(
        args.hydra_check
            .clone()
            .unwrap_or_else(|| String::from("hydra-check")),
    )
    .arg(&args.package)
    .arg("--json")
    .output()?
    .stdout;
    let stdout = String::from_utf8_lossy(&stdout);
    let response: HashMap<String, Vec<Job>> = serde_json::from_str(&stdout)?;
    let job = response
        .into_iter()
        .next()
        .ok_or(eyre!("hydra-check response contained no packages"))?
        .1
        .into_iter()
        .find(|job| job.success == true)
        .ok_or(eyre!("there are no succeeding builds on hydra"))?;
    let build_response: Build = serde_json::from_str(
        &Client::builder()
            .build()?
            .get(&format!("https://hydra.nixos.org/build/{}", job.build_id))
            .header("Accept", "application/json")
            .send()?
            .text()?,
    )?;
    let eval_response: Eval = serde_json::from_str(
        &Client::builder()
            .build()?
            .get(&format!(
                "https://hydra.nixos.org/eval/{}",
                build_response.jobsetevals[0]
            ))
            .header("Accept", "application/json")
            .send()?
            .text()?,
    )?;
    let input = eval_response
        .jobsetevalinputs
        .get("nixpkgs")
        .ok_or(eyre!("package does not use nixpkgs in input"))?;
    // println!("{:?}", input);
    let re = Regex::new("https://github.com/(.*?)/(.*?).git").expect("regex didn't build wtf");
    let uri = input
        .uri
        .clone()
        .ok_or(eyre!("nixpkgs input does not have uri"))?;
    let caps = re
        .captures(&uri)
        .ok_or(eyre!("package source uri did not match github.com regex"))?;
    let tarball_url = format!(
        "https://github.com/{}/{}/archive/{}.tar.gz",
        &caps[1],
        &caps[2],
        input
            .revision
            .clone()
            .ok_or(eyre!("nixpkgs input does not have uri"))?
    );
    let npf_output = Command::new("nix-prefetch-url")
        .arg("--unpack")
        .arg(&tarball_url)
        .stderr(Stdio::inherit())
        .output()?;
    let hash = String::from_utf8_lossy(&npf_output.stdout);
    Ok(Package {
        name: args.package.to_string(),
        url: tarball_url,
        sha256: hash.trim().to_string(),
    })
}

fn existing_packages(path: impl AsRef<Path>) -> Result<Vec<Package>> {
    let current = std::fs::read_to_string(path.as_ref()).unwrap_or(String::new());
    Ok(current
        .split('\n')
        .take_while(|line| line.starts_with("#"))
        .filter_map(|line| line.strip_prefix("# "))
        .map(|line| line.split(" "))
        .filter_map(|mut split| Some((split.next()?, split.next()?, split.next()?)))
        .map(|(name, url, hash)| (name.to_owned(), url.to_owned(), hash.to_owned()))
        .map(|(name, url, sha256)| Package { name, url, sha256 })
        .collect::<Vec<_>>())
}


fn pin(args: &Args) -> Result<()> {
    let new_package = get_package(args)?;
    let mut existing = existing_packages(&args.output)?;
    existing.push(new_package);
    std::fs::write(&args.output, &format!("{}", Overlay { packages: existing }))?;
    Ok(())
}

fn unpin(args: &Args) -> Result<()> {
    std::fs::write(&args.output, &format!("{}", Overlay { packages: existing_packages(&args.output)?.into_iter().filter(|pkg| pkg.name != args.package).collect::<Vec<_>>() }))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Action::Pin => pin(&args)?,
        Action::Unpin => unpin(&args)?,
    };
    Ok(())
}

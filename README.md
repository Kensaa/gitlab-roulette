# Gitlab Roulette

Let's go gambling !!!

Assign a list of issues randomly to selected project members

## Installation

```
cargo install --git https://github.com/Kensaa/gitlab-roulette
```

## Usage

```
gitlab-roulette --help
```

## Config File

The default config file (can be changed using the --config-file arg) is `./gitlab-roulette.toml`
The config file can contain the following fields :

- **url** : URL to the repo
- **token** : Gitlab token to use to interact with the repo
- **issues** : List of issue id to assign
- **members** : List of member username to assign the issues to

If the url given points to a valid gitlab but the project does not exist, you will be prompted to select a project from those you can access using your token.

## Args

Similar to the config, see `gitlab-roulette --help`

#![feature(exit_status_error)]

use std::{
    io::{stdin, stdout, Stdout},
    process::Command,
    time::Duration,
};

use anyhow::Context;
use crossterm::{
    cursor::{MoveRight, RestorePosition, SavePosition},
    event::{Event, EventStream, KeyCode},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType},
};
use futures::{future::Fuse, pin_mut, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use tinytemplate::TinyTemplate;

const QUERY_PREFIX: &str = "Search > ";

#[derive(Deserialize)]
struct Config {
    query_command: QueryCommand,
    timeout_millis: u64,
    display_template: String,
}

#[derive(Deserialize)]
struct QueryCommand {
    executable: String,
    args: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = serde_json::from_reader::<_, Config>(stdin())
        .context("unable to load config from stdin")?;
    if let Some(identifier) = run(config).await? {
        eprintln!("{}", identifier);
    }

    Ok(())
}

async fn run(config: Config) -> anyhow::Result<Option<String>> {
    let mut out = stdout();
    enable_raw_mode()?;
    let mut event_stream = EventStream::new();

    execute!(out, Print(QUERY_PREFIX), SavePosition)?;
    let mut query = String::new();
    let mut selected_index = 0;
    let mut current_result: Option<SearchResult> = None;

    let search_future = Fuse::terminated();
    pin_mut!(search_future);
    let result = loop {
        let mut next_event = event_stream.next().fuse();

        futures::select! {
            maybe_event = next_event => {
                match maybe_event {
                    Some(Ok(event)) => {
                        match event {
                            Event::Key(key) => {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        query.push(c);
                                        update_query(&mut out, &query)?;
                                        search_future.set(Box::new(search(&config, query.clone())).fuse());
                                    }

                                    KeyCode::Backspace => {
                                        query.pop();
                                        update_query(&mut out, &query)?;
                                        search_future.set(Box::new(search(&config, query.clone())).fuse());
                                    }

                                    KeyCode::Up => {
                                        if let Some(result) = current_result.as_ref() {
                                            let num_results = result.results.len();
                                            if num_results > 0 {
                                                selected_index = (selected_index + num_results - 1) % num_results;
                                                update_results(&mut out, &config, &current_result, selected_index)?;
                                            }
                                        }
                                    }

                                    KeyCode::Down => {
                                        if let Some(result) = current_result.as_ref() {
                                            let num_results = result.results.len();
                                            if num_results > 0 {
                                                selected_index = (selected_index + 1) % num_results;
                                                update_results(&mut out, &config, &current_result, selected_index)?;
                                            }
                                        }
                                    }

                                    KeyCode::Esc => {
                                        break None;
                                    }

                                    KeyCode::Enter => {
                                        if let Some(result) = current_result.as_ref() {
                                            if let Some(entry) = result.results.get(selected_index) {
                                                break Some(entry.identifier.clone());
                                            }
                                        }
                                    }

                                    _ => {}
                                }
                            }

                            Event::Resize(_, _) => {
                                update_results(&mut out, &config, &current_result, selected_index)?;
                            }

                            _ => {}
                        }
                    }

                    Some(Err(error)) => {
                        return Err(error.into());
                    }

                    None => {}
                }
            }

            search_result = search_future => {
                match search_result {
                    Ok(result) => {
                        current_result.replace(result);
                        update_results(&mut out, &config, &current_result, selected_index)?;
                        selected_index = 0;
                    }

                    Err(err) => {
                        execute!(out, Print(format_args!("\r\n{}", err)))?;
                        current_result.take();
                        update_results(&mut out, &config, &current_result, selected_index)?;
                        selected_index = 0;
                    }
                }
            }
        }
    };
    
    execute!(out, Print("\r"), Clear(ClearType::FromCursorDown))?;                
    disable_raw_mode()?;
    Ok(result)
}

#[derive(Deserialize)]
struct SearchResult {
    results: Vec<SearchResultEntry>,
}

#[derive(Deserialize)]
struct SearchResultEntry {
    confidence: f64,
    identifier: String,
    title: String,
}

fn update_query(out: &mut Stdout, query: &str) -> anyhow::Result<()> {
    execute!(
        out,
        RestorePosition,
        Print("\r"),
        MoveRight(QUERY_PREFIX.len() as u16),
        Clear(ClearType::UntilNewLine),
        Print(query),
        SavePosition
    )?;
    Ok(())
}

async fn search(config: &Config, query: String) -> anyhow::Result<SearchResult> {
    #[derive(Serialize)]
    struct Context {
        query: String,
        query_escaped: String,
    }

    tokio::time::sleep(Duration::from_millis(config.timeout_millis)).await;

    let context = Context {
        query_escaped: query.escape_debug().to_string(),
        query,
    };

    let template = |template_string| Template::new(template_string)?.render(&context);

    let process_output = Command::new(template(&config.query_command.executable)?)
        .args(
            config
                .query_command
                .args
                .iter()
                .map(|arg| template(arg))
                .collect::<anyhow::Result<Vec<_>>>()?,
        )
        .output()?;

    process_output
        .status
        .exit_ok()
        .map_err(|err| {
            let error = std::str::from_utf8(&process_output.stderr)
                .unwrap_or("unable to decode stderr as utf-8");
            anyhow::anyhow!("{error}, status error {err}")
        })
        .and_then(|_| {
            let result = serde_json::from_slice::<SearchResult>(&process_output.stdout)?;
            Ok(result)
        })
}

fn update_results(
    out: &mut Stdout,
    config: &Config,
    result: &Option<SearchResult>,
    selected_index: usize,
) -> anyhow::Result<()> {
    #[derive(Serialize)]
    struct Context<'a> {
        identifier: &'a str,
        title: &'a str,
        confidence: f64,
        index: usize,
        one_based_index: usize,
        display_index: usize,
        one_based_display_index: usize,
    }
    
    execute!(out, Clear(ClearType::FromCursorDown))?;
    let _guard = RestorePositionRAII;
    if let Some(result) = result {
        let num_results = result.results.len();
        if num_results == 0 {
            queue!(out, Print("\r\nno entries found"))?;
        } else {
            let term_height = size()?.1;
            let max_results_shown: usize = (term_height.max(2) - 2).into();
            let display_template = Template::new(&config.display_template)?;
            for index in 0..num_results.min(max_results_shown) {
                queue!(out, Print("\r\n"))?;
                if index == 0 {
                    queue!(
                        out,
                        SetForegroundColor(Color::Black),
                        SetBackgroundColor(Color::White)
                    )?;
                }
                let entry_index = (selected_index + index) % num_results;
                let entry = result.results.get(entry_index).unwrap();
                let display_string = display_template.render(&Context {
                    identifier: &entry.identifier,
                    title: &entry.title,
                    confidence: entry.confidence,
                    index: entry_index,
                    one_based_index: entry_index + 1,
                    display_index: index,
                    one_based_display_index: index + 1,
                })?;
                queue!(out, Clear(ClearType::UntilNewLine), Print(display_string))?;
                if index == 0 {
                    queue!(out, ResetColor)?;
                }
            }
        }
    }

    Ok(())
}

struct Template<'a> {
    template: TinyTemplate<'a>,
}

impl<'a> Template<'a> {
    pub fn new(template_string: &'a str) -> anyhow::Result<Self> {
        let mut template = TinyTemplate::new();
        template.add_template("main", template_string)?;
        Ok(Self { template })
    }

    pub fn render<C: serde::Serialize>(&self, context: &C) -> anyhow::Result<String> {
        Ok(self.template.render("main", context)?)
    }
}

struct RestorePositionRAII;

impl Drop for RestorePositionRAII {
    fn drop(&mut self) {
        execute!(stdout(), RestorePosition).expect("unable to restore cursor position");
    }
}

/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fs;
use std::sync::Arc;

use clap::Args;
use clap::Subcommand;
use common::FeatureFlag;
use common::Rollout;
use common::RolloutRange;
use log::info;
use lsp_types::CodeActionOrCommand;
use lsp_types::TextEdit;
use lsp_types::Url;
use relay_compiler::config::Config;
use relay_transforms::Programs;
use relay_transforms::disallow_required_on_non_null_field;
use relay_transforms::fragment_alias_directive;

#[derive(Subcommand, Debug, Clone)]
pub enum AvailableCodemod {
    /// Marks unaliased conditional fragment spreads as @dangerously_unaliased_fixme
    MarkDangerousConditionalFragmentSpreads(MarkDangerousConditionalFragmentSpreadsArgs),

    /// Removes @required directives from non-null fields within @throwOnFieldError fragments and operations.
    RemoveUnnecessaryRequiredDirectives,
}

#[derive(Args, Debug, Clone)]
pub struct MarkDangerousConditionalFragmentSpreadsArgs {
    /// Specify a percentage of fragments to codemod. If a number is provided,
    /// the first n percentage of fragments will be codemodded. If a range (`20-30`) is
    /// provided, then fragments between the start and end of the range will be codemodded.
    #[clap(long, short, value_parser=valid_percent, default_value = "100")]
    pub rollout_percentage: FeatureFlag,
}

pub async fn run_codemod(
    programs: Vec<Arc<Programs>>,
    config: Arc<Config>,
    codemod: AvailableCodemod,
) -> Result<(), std::io::Error> {
    let diagnostics = programs
        .iter()
        .flat_map(|programs| match &codemod {
            AvailableCodemod::MarkDangerousConditionalFragmentSpreads(opts) => {
                match fragment_alias_directive(&programs.source, &opts.rollout_percentage) {
                    Ok(_) => vec![],
                    Err(e) => e,
                }
            }
            AvailableCodemod::RemoveUnnecessaryRequiredDirectives => {
                match disallow_required_on_non_null_field(&programs.reader) {
                    Ok(_) => vec![],
                    Err(e) => e,
                }
            }
        })
        .collect::<Vec<_>>();

    let actions = relay_lsp::diagnostics_to_code_actions(&config.root_dir, &diagnostics);

    info!(
        "Codemod {:?} ran and found {} changes to make.",
        codemod,
        actions.len()
    );
    apply_actions(actions)?;
    Ok(())
}

fn apply_actions(actions: Vec<CodeActionOrCommand>) -> Result<(), std::io::Error> {
    let mut collected_changes = std::collections::HashMap::new();

    // Collect all the changes into a map of file-to-list-of-changes
    for action in actions {
        if let CodeActionOrCommand::CodeAction(code_action) = action {
            if let Some(changes) = code_action.edit.unwrap().changes {
                for (file, changes) in changes {
                    collected_changes
                        .entry(file)
                        .or_insert_with(Vec::new)
                        .extend(changes);
                }
            }
        }
    }

    for (file, mut changes) in collected_changes {
        sort_changes(&file, &mut changes)?;

        // Read file into memory and apply changes
        let file_contents: String = fs::read_to_string(file.path())?;
        let mut lines: Vec<String> = file_contents.lines().map(|s| s.to_string()).collect();
        for change in &changes {
            let line = change.range.start.line as usize;
            let mut new_line = String::new();
            new_line.push_str(&lines[line][..change.range.start.character as usize]);
            new_line.push_str(&change.new_text);
            new_line.push_str(&lines[line][change.range.end.character as usize..]);
            lines[line] = new_line;
        }

        // Write file back out
        let new_file_contents = lines.join("\n");
        fs::write(file.path(), new_file_contents)?;

        info!("Applied {} changes to {}", changes.len(), file.path());
    }
    Ok(())
}

fn sort_changes(url: &Url, changes: &mut Vec<TextEdit>) -> Result<(), std::io::Error> {
    // Now we have all the changes for this file. Sort them by position within the file, end of file first
    // This way the changes are applied in reverse order, so we don't have to worry about altering the positions of the remaining changes
    changes.sort_by(|a, b| b.range.start.cmp(&a.range.start));

    // Verify none of the changes overlap
    let mut prev_change: Option<&TextEdit> = None;
    for change in changes {
        if let Some(prev_change) = prev_change {
            if change.range.end.line > prev_change.range.start.line
                || (change.range.end.line == prev_change.range.start.line
                    && change.range.end.character > prev_change.range.start.character)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "Codemod produced changes that overlap: File {}, changes: {:?} vs {:?}",
                        url.path(),
                        change,
                        prev_change
                    ),
                ));
            }
        }
        prev_change = Some(change);
    }
    Ok(())
}

fn valid_percent(s: &str) -> Result<FeatureFlag, String> {
    // If the string is a range of the form "x-y", where x and y are numbers, return the range
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 2 {
        let start = parts[0].parse::<u8>().map_err(|_| {
            "Expected the value on the left of the rollout range to be a number".to_string()
        })?;
        let end = parts[1].parse::<u8>().map_err(|_| {
            "Expected the value on the right of the rollout range to be a number".to_string()
        })?;
        if (0..=100).contains(&start) && (0..=100).contains(&end) && start <= end {
            Ok(FeatureFlag::RolloutRange {
                rollout: RolloutRange { start, end },
            })
        } else {
            Err("numbers must be between 0 and 100, inclusive, and the first number must be less than or equal to the second".to_string())
        }
    } else {
        // turn s into a u8
        let s = s.parse::<u8>().map_err(|_| "not a number".to_string())?;
        // check if s is less than 100
        if (0..=100).contains(&s) {
            Ok(FeatureFlag::Rollout {
                rollout: Rollout(Some(s)),
            })
        } else {
            Err("number must be between 0 and 100, inclusive".to_string())
        }
    }
}

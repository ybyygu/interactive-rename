// [[file:../rename.note::*imports][imports:1]]
use gut::prelude::*;
// imports:1 ends here

// [[file:../rename.note::*mods][mods:1]]

// mods:1 ends here

// [[file:../rename.note::*base][base:1]]
mod base {
    use std::path::{Path, PathBuf};

    /// Rename `source` to `dest`
    #[derive(Debug)]
    pub struct Rename {
        pub source: PathBuf,
        pub dest: PathBuf,
    }

    impl Rename {
        pub fn new<P: AsRef<Path>>(source: P, dest: P) -> Self {
            Self {
                source: source.as_ref().into(),
                dest: dest.as_ref().into(),
            }
        }
    }

    impl std::fmt::Display for Rename {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Renaming: {:?} → {:?}", self.source, self.dest)
        }
    }
}
// base:1 ends here

// [[file:../rename.note::*edit][edit:1]]
/// Call EDITOR (vi or emacs) to edit file renamings
fn interactive_edit(txt: &str) -> Result<String> {
    let tmpfile = tempfile::NamedTempFile::new()?;
    gut::fs::write_to_file(&tmpfile, txt)?;

    let editor = std::env::var("EDITOR").unwrap_or("vi".to_string());
    println!("Edit renamings using {}", editor);

    let _ = std::process::Command::new(editor)
        .arg(tmpfile.path())
        .status()?;
    let new = gut::fs::read_file(tmpfile)?;

    Ok(new)
}
// edit:1 ends here

// [[file:../rename.note::*find][find:1]]
mod find {
    use super::*;
    use skim::prelude::*;

    pub fn find_files(query_str: Option<&str>) -> Vec<String> {
        let options = SkimOptionsBuilder::default()
            .multi(true)
            .prompt(Some("search> "))
            .query(query_str)
            .build()
            .unwrap();

        let selected_items = Skim::run_with(&options, None)
            .map(|out| out.selected_items)
            .unwrap_or_else(|| Vec::new());

        selected_items
            .iter()
            .map(|x| x.output().to_string())
            .collect()
    }
}
// find:1 ends here

// [[file:../rename.note::*rename][rename:1]]
mod rename {
    use super::*;
    use base::Rename;

    impl Rename {
        /// Direct renaming. Return false if found naming conflict
        fn apply(&self) -> Result<bool> {
            if self.dest.exists() {
                // found file name conflict
                Ok(false)
            } else {
                // correctly renamed?
                std::fs::rename(&self.source, &self.dest)?;
                // yes
                Ok(true)
            }
        }
    }

    /// Execute file renaming rules
    pub fn apply_file_renaming_rules(rules: &[Rename]) -> Result<()> {
        // Sanity check
        let n = rules.len();
        // 1. each rule should have unique source file
        let s: std::collections::HashSet<_> = rules.iter().map(|r| &r.source).collect();
        if s.len() != n {
            bail!("Found duplicte items in source files!");
        }

        // 2. each rule should have unique dest file
        let s: std::collections::HashSet<_> = rules.iter().map(|r| &r.dest).collect();
        if s.len() != n {
            bail!("Found duplicte items in dest files!");
        }

        let remained: Vec<_> = rules
            .iter()
            .filter_map(|r| {
                let done = r.apply().ok()?;
                if !done {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        // handle rules that involving renaming conflicts
        if !remained.is_empty() {
            resolve_renaming_conflicts(&remained)?;
        }

        Ok(())
    }

    // Possible scenery
    // a => b
    // b => a
    fn resolve_renaming_conflicts(rules: &[&Rename]) -> Result<()> {
        println!("Resolving {} renaming conflicts ...", rules.len());
        let tmp_files: Result<Vec<_>> = rules
            .iter()
            .map(|r| {
                if let Some(d) = r.source.parent() {
                    // create a temp file and then remove it
                    // we only need a valid temp file name
                    let p = tempfile::NamedTempFile::new_in(d)?
                        .into_temp_path()
                        .to_path_buf();
                    Ok(p)
                } else {
                    bail!("failed to get parent dir: {:?}", r.source);
                }
            })
            .collect();

        let tmp_files = tmp_files.context("tmp file for avoiding naming conflicts")?;
        // renaming files stage 1: move `source` file to a temp location
        for (r, tmp) in rules.iter().zip(tmp_files.iter()) {
            std::fs::rename(&r.source, tmp)
                .with_context(|| format!("mv {:?} {:?}", r.source, tmp))?;
        }
        // renaming files stage 2: move the temp file to `dest`
        for (r, tmp) in rules.iter().zip(tmp_files.iter()) {
            std::fs::rename(tmp, &r.dest).with_context(|| format!("mv {:?} {:?}", tmp, r.dest))?;
        }

        Ok(())
    }

    // find file renaming rules line by line between old text and new text
    pub fn find_file_renaming_rules(old: &str, new: &str) -> Vec<Rename> {
        if old == new {
            println!("found no changes!");
            return vec![];
        }
        if old.lines().count() != new.lines().count() {
            error!("found invalid changes!");
            return vec![];
        }

        old.lines()
            .zip(new.lines())
            .filter_map(|(source, dest)| {
                if source == dest {
                    None
                } else {
                    Rename::new(source, dest).into()
                }
            })
            .collect()
    }

    #[test]
    fn test_file_renamings() {
        let renames = find_file_renaming_rules("a", "b");
        assert_eq!(renames.len(), 1);

        let old = "a\nb";
        let new = "b\nc";
        let renames = find_file_renaming_rules(old, new);
        assert_eq!(renames.len(), 2);

        let old = "a";
        let new = "b\nc";
        let renames = find_file_renaming_rules(old, new);
        assert_eq!(renames.len(), 0);

        let old = "a\nb";
        let new = "a\nc";
        let renames = find_file_renaming_rules(old, new);
        assert_eq!(renames.len(), 1);
    }
}
// rename:1 ends here

// [[file:../rename.note::*entry][entry:1]]
use structopt::*;

#[derive(StructOpt)]
/// Rename files interactively
struct Cli {
    query: Option<String>,
}

pub fn enter_main() -> Result<()> {
    let args = Cli::from_args();

    // 1. call skim, generate selected source file names
    let files = self::find::find_files(args.query.as_deref());

    // 2. call vim, interactive edit
    let s_old = files.join("\n");
    let s_new = interactive_edit(&s_old)?;

    // 3. compare changes, generate renaming rules
    let rules = self::rename::find_file_renaming_rules(&s_old, &s_new);

    // 4. apply renaming, and resolve conflicts if necessary
    if !rules.is_empty() {
        for r in rules.iter() {
            println!("{}", r);
        }
        let yes = promptly::prompt_default("Continue to rename?", true)?;
        if yes {
            self::rename::apply_file_renaming_rules(&rules)?;
        } else {
            eprintln!("canceled");
        }
    }
    println!("Done!");

    Ok(())
}
// entry:1 ends here

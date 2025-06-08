use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::db;

#[derive(Parser)]
#[command(name = "musiq")]
#[command(about = "A CLI music library", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(name = "song", alias = "s", about = "Create or edit a song")]
    Song(SongArgs),

    #[command(name = "tag", alias = "t", about = "Tag operations")]
    Tag {
        tag: String,
    },

    #[command(name = "export", alias = "e", about = "Export a playlist from specified tags")]
    Export {
        // Tag conditions, e.g. energy>=7 mood<5 background=3
        tags: Vec<String>,
    },
}

#[derive(Args)]
pub struct SongArgs {
    pub path: PathBuf,

    #[command(subcommand)]
    pub action: Option<SongSubcommand>,
}

#[derive(Subcommand)]
pub enum SongSubcommand {
    #[command(about = "Tag a song with key=value pairs")]
    Tag {
        // Tags and values, e.g. a=1 b=2
        tags: Vec<String>,
    },
    #[command(about = "Add a song to the db via its path")]
    Add {
        path: String,
    },
    #[command(about = "Remove a song from the db via its path")]
    Remove {
        path: String,
    },
}

// Helper function to parse tag conditions like "energy>=7" into (tag_name, value, operator)
fn parse_tag_condition(condition: &str) -> Result<(String, u8, String), String> {
    // Try different operators in order of specificity (longer ones first)
    let operators = [">=", "<=", "!=", ">", "<", "="];
    
    for op in &operators {
        if let Some(pos) = condition.find(op) {
            let tag_name = condition[..pos].trim().to_string();
            let value_str = condition[pos + op.len()..].trim();
            
            if tag_name.is_empty() {
                return Err(format!("Empty tag name in condition: {}", condition));
            }
            
            match value_str.parse::<u8>() {
                Ok(value) if value <= 9 => {
                    return Ok((tag_name, value, op.to_string()));
                }
                Ok(_) => {
                    return Err(format!("Tag value must be between 0 and 9 in: {}", condition));
                }
                Err(_) => {
                    return Err(format!("Invalid numeric value in: {}", condition));
                }
            }
        }
    }
    
    Err(format!("No valid operator found in condition: {} (use =, >, <, >=, <=, !=)", condition))
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Song(song_args) => {
            let path = song_args.path.to_string_lossy().to_string();
            match song_args.action {
                Some(SongSubcommand::Tag { tags }) => {
                    for tag_arg in tags {
                        match parse_tag_condition(&tag_arg) {
                            Ok((name, value, operator)) => {
                                if operator != "=" {
                                    eprintln!("Song tagging only supports '=' operator, got: {}", tag_arg);
                                    continue;
                                }
                                db::add_tag(&name).expect("Failed to add tag");
                                db::tag_song(&path, &name, value).expect("Failed to tag");
                            }
                            Err(error) => {
                                eprintln!("Error parsing tag: {}", error);
                            }
                        }
                    }
                }
                Some(SongSubcommand::Add { path }) => {
                    db::add_song(&path).expect(&format!("Failed to add song @ {path}"));
                }
                Some(SongSubcommand::Remove { path }) => {
                    db::remove_song(&path).expect(&format!("Failed to remove song @ {path}"));
                }
                None => {
                    eprintln!("No song subcommand specified, exiting...");
                }
            }
        }
        Commands::Tag { tag: _ } => {
            println!("Tag command stub");
        }
        Commands::Export { tags } => {
            if tags.is_empty() {
                eprintln!("No tag conditions specified");
                return;
            }

            // Parse tag conditions
            let mut conditions = Vec::new();
            for tag_condition in tags {
                match parse_tag_condition(&tag_condition) {
                    Ok(condition) => conditions.push(condition),
                    Err(error) => {
                        eprintln!("Error parsing tag condition: {}", error);
                        return;
                    }
                }
            }

            // Query the database
            match db::query_songs(&conditions) {
                Ok(songs) => {
                    if songs.is_empty() {
                        println!("No songs found matching the specified conditions");
                    } else {
                        println!("Found {} songs:", songs.len());
                        for song in songs {
                            println!("{}", song.path);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Database error: {}", error);
                }
            }
        }
    }
}

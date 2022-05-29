use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use chrono::prelude::*;
use clap::Parser;
use num_format::ToFormattedString;
use rusqlite::{Connection, OpenFlags};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Queue {
    New,
    Learning,
    Review,
}

impl Queue {
    fn class(&self) -> &'static str {
        match self {
            Queue::New => "new",
            Queue::Learning => "learning",
            Queue::Review => "review",
        }
    }
}

impl TryFrom<u8> for Queue {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Queue::New),
            1 | 3 => Ok(Queue::Learning),
            2 => Ok(Queue::Review),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Card {
    fields: Vec<String>,
    queue: Queue,
    reps: u32,
    lapses: u32,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Path to the Anki collection database
    #[clap(short, long)]
    collection_path: PathBuf,

    /// Anki deck ID to generate statistics for
    #[clap(short, long)]
    deck_id: String,

    /// Path to the output HTML file to generate
    #[clap(short)]
    output: Option<PathBuf>,

    /// Path to the template HTML file to use
    #[clap(short)]
    template: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let conn = Connection::open_with_flags(args.collection_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut cards_stmt = conn.prepare(&format!(
        "SELECT notes.flds, cards.queue, cards.reps, cards.lapses \
            FROM cards \
            INNER JOIN notes ON notes.id = cards.nid \
            WHERE cards.did = {deck_id}
            ORDER BY notes.id",
        deck_id = args.deck_id
    ))?;

    let cards_iter = cards_stmt.query_map([], |row| {
        Ok(Card {
            fields: row
                .get::<_, String>(0)?
                .split('\x1f')
                .map(|string| string.to_owned())
                .collect::<Vec<String>>(),
            queue: row
                .get::<_, u8>(1)?
                .try_into()
                .expect("cannot convert unexpected queue value"),
            reps: row.get(2)?,
            lapses: row.get(3)?,
        })
    })?;

    let mut n_cards: usize = 0;
    let mut n_learned: usize = 0;
    let mut cards = String::new();

    for card in cards_iter {
        let card = card?;
        if card.queue == Queue::Review {
            n_learned += 1;
        }
        n_cards += 1;
        // println!("{:?}", card);
        cards.push_str(&format!(
            "<a href='https://jisho.org/search/{front}' class='card-link'>\
                <div class='card card-{queue_class}'>{front}\
                    <div class='card-hover'>\
                        <div class='card-meaning'>{reading}; {english}</div>
                        reviews: {reps}<br/>
                        lapses: {lapses}<br/>
                    </div>\
                </div>\
            </a>\n",
            queue_class = card.queue.class(),
            front = card.fields[0],
            reps = card.reps,
            lapses = card.lapses,
            reading = card.fields[1],
            english = card.fields[2],
        ));
    }

    let learned_percentage: f64 = n_learned as f64 / n_cards as f64 * 100.0;
    println!(
        "learned {}/{} ({:.2}%)",
        n_learned, n_cards, learned_percentage
    );

    let mut tokens = HashMap::new();
    tokens.insert(
        "n_learned",
        n_learned.to_formatted_string(&num_format::Locale::en),
    );
    tokens.insert(
        "n_cards",
        n_cards.to_formatted_string(&num_format::Locale::en),
    );
    tokens.insert(
        "learned_percentage_pretty",
        format!("{:.2}", learned_percentage),
    );
    tokens.insert("cards", cards);
    tokens.insert("now", Local::now().to_rfc3339());

    let template_path = args.template.unwrap_or("./template.html".parse().unwrap());
    let mut template = std::fs::read_to_string(template_path)?;
    for (token, value) in tokens.iter() {
        template = template.replace(&format!("{{{}}}", token), value);
    }

    let output_path = args.output.unwrap_or("./core2300.html".parse().unwrap());
    println!("writing generated html to {:?}", output_path);
    std::fs::write(output_path, template)?;

    Ok(())
}

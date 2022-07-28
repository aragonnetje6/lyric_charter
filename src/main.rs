use std::collections::HashMap;

use gloo::file::callbacks::{read_as_text, FileReader};
use gloo::file::File;
use regex::Regex;
use web_sys::{console, HtmlInputElement};
use yew::prelude::*;

use crate::KeyPressEvent::{Note, Special, TextEvent};
use crate::LyricEvent::{Lyric, PhraseEnd, PhraseStart, Section};
use crate::TempoEvent::{Anchor, Beat, TimeSignature};

enum Msg {
    Files(Vec<File>),
    Loaded(String, String),
}

trait ChartEvent {
    fn get_timestamp(&self) -> u32;
}

#[derive(Debug)]
enum LyricEvent {
    PhraseStart { timestamp: u32 },
    PhraseEnd { timestamp: u32 },
    Lyric { timestamp: u32, text: String },
    Section { timestamp: u32, text: String },
}

impl ChartEvent for LyricEvent {
    fn get_timestamp(&self) -> u32 {
        match self {
            PhraseStart { timestamp } => *timestamp,
            PhraseEnd { timestamp } => *timestamp,
            Lyric { timestamp, .. } => *timestamp,
            Section { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Debug)]
enum KeyPressEvent {
    Note {
        timestamp: u32,
        duration: u32,
        key: u8,
    },
    Special {
        timestamp: u32,
        special_type: u32,
        duration: u32,
    },
    TextEvent {
        timestamp: u32,
        content: String,
    },
}

impl ChartEvent for KeyPressEvent {
    fn get_timestamp(&self) -> u32 {
        match self {
            Note { timestamp, .. } => *timestamp,
            Special { timestamp, .. } => *timestamp,
            TextEvent { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Debug)]
enum TempoEvent {
    Beat {
        timestamp: u32,
        milli_bpm: u64,
    },
    TimeSignature {
        timestamp: u32,
        time_signature: (u32, u32),
    },
    Anchor {
        timestamp: u32,
        song_microseconds: u64,
    },
}

impl ChartEvent for TempoEvent {
    fn get_timestamp(&self) -> u32 {
        match self {
            Beat { timestamp, .. } => *timestamp,
            TimeSignature { timestamp, .. } => *timestamp,
            Anchor { timestamp, .. } => *timestamp,
        }
    }
}

struct Chart {
    properties: HashMap<String, String>,
    lyrics: Vec<LyricEvent>,
    sync_track: Vec<TempoEvent>,
    key_presses: HashMap<String, Vec<KeyPressEvent>>,
}

impl Chart {
    fn from(chart_file: String) -> Self {
        // initialise regexes
        let header_regex = Regex::new("\\[(?P<header>[^]]+)]").unwrap();
        let properties_regex = Regex::new(" {2}(?P<property>[^ =]+) = (?P<content>.+)").unwrap();
        let sync_track_regex = Regex::new(
            " {2}(?P<timestamp>\\d+) = (?P<type>\\w+) (?P<number1>\\d+)( (?P<number2>\\d+))?",
        )
        .unwrap();
        let lyrics_regex =
            Regex::new(" {2}(?P<timestamp>\\d+) = E \"(?P<type>[^ \"]+)( (?P<content>[^\"]+))?\"")
                .unwrap();
        let notes_regex =
            Regex::new(" {2}(?P<timestamp>\\d+) = (?P<type>[NSE]) (?P<key>.) (?P<duration>\\d)?")
                .unwrap();

        // declare output variables
        let mut properties = HashMap::new();
        let mut lyrics = vec![];
        let mut sync_track = vec![];
        let mut key_presses = HashMap::new();

        // decode file
        for section in chart_file.split("}") {
            let header = match header_regex.find(&section) {
                None => continue,
                Some(x) => x.as_str().replace("[", "").replace("]", ""),
            };
            match header.as_str() {
                "Song" => Self::decode_properties(&properties_regex, &mut properties, section),
                "SyncTrack" => Self::decode_sync_track(&sync_track_regex, &mut sync_track, section),
                "Events" => Self::decode_lyrics(&lyrics_regex, &mut lyrics, section),
                &_ => Self::decode_notes(&notes_regex, &mut key_presses, section, header),
            }
        }
        Self {
            properties,
            sync_track,
            lyrics,
            key_presses,
        }
    }

    fn decode_lyrics(lyrics_regex: &Regex, lyrics: &mut Vec<LyricEvent>, section: &str) {
        lyrics_regex.captures_iter(section).for_each(|captures| {
            lyrics.push(match &captures["type"] {
                "section" => Section {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                    text: captures["content"].to_owned(),
                },
                "lyric" => Lyric {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                    text: captures["content"].to_owned(),
                },
                "phrase_end" => PhraseEnd {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                },
                "phrase_start" => PhraseStart {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                },
                err => panic!("unrecognised lyric event type {}", err),
            });
        })
    }

    fn decode_sync_track(regex: &Regex, sync_track: &mut Vec<TempoEvent>, section: &str) {
        regex.captures_iter(section).for_each(|captures| {
            sync_track.push(match &captures["type"] {
                "A" => Anchor {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                    song_microseconds: captures["number1"].parse().expect("parsing error"),
                },
                "B" => Beat {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                    milli_bpm: captures["number1"].parse().expect("parsing error"),
                },
                "TS" => TimeSignature {
                    timestamp: captures["timestamp"].parse().expect("parsing error"),
                    time_signature: (
                        captures["number1"].parse().expect("parsing error"),
                        2_u32.pow(if let Some(x) = captures.name("number2") {
                            x.as_str().parse().expect("parsing error")
                        } else {
                            2
                        }) as u32,
                    ),
                },
                err => panic!("unknown SyncTrack event {}", err),
            })
        });
    }

    fn decode_properties(regex: &Regex, properties: &mut HashMap<String, String>, section: &str) {
        regex.captures_iter(&section).for_each(|captures| {
            properties.insert(
                captures["property"].to_owned(),
                captures["content"].to_owned(),
            );
        })
    }

    fn decode_notes(
        regex: &Regex,
        key_presses: &mut HashMap<String, Vec<KeyPressEvent>>,
        section: &str,
        header: String,
    ) {
        key_presses.insert(
            header.replace("[", "").replace("]", "").to_owned(),
            regex
                .captures_iter(&section)
                .map(|captures| match &captures["type"] {
                    "N" => Note {
                        timestamp: captures["timestamp"].parse().expect("parsing error"),
                        duration: captures["duration"].parse().expect("parsing error"),
                        key: captures["key"].parse().expect("parsing error"),
                    },
                    "S" => Special {
                        timestamp: captures["timestamp"].parse().expect("parsing error"),
                        special_type: captures["key"].parse().expect("parsing error"),
                        duration: captures["duration"].parse().expect("parsing error"),
                    },
                    "E" => TextEvent {
                        timestamp: captures["timestamp"].parse().expect("parsing error"),
                        content: captures["key"].to_owned(),
                    },
                    x => panic!("unrecognised keypress type {}", x),
                })
                .collect(),
        );
    }
}

struct Model {
    readers: HashMap<String, FileReader>,
    chart: Option<Chart>,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        console::log_1(&"Hello using web-sys".into());
        Self {
            readers: HashMap::default(),
            chart: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Files(files) => {
                for file in files.into_iter() {
                    let file_name = file.name();
                    let task = {
                        let file_name = file_name.clone();
                        let link = ctx.link().clone();
                        read_as_text(&file, move |res| {
                            link.send_message(Msg::Loaded(
                                file_name,
                                res.unwrap_or_else(|e| e.to_string()),
                            ))
                        })
                    };
                    self.readers.insert(file_name, task);
                }
                true
            }
            Msg::Loaded(file_name, data) => {
                self.readers.remove(&file_name);
                self.chart = Some(Chart::from(data));
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let _link = ctx.link();
        html! {
            <div>
                <input type="file" accept=".chart" onchange={ctx.link().callback(move |e: Event| {
                            let mut result = Vec::new();
                            let input: HtmlInputElement = e.target_unchecked_into();

                            if let Some(files) = input.files() {
                                let files = js_sys::try_iter(&files)
                                    .unwrap()
                                    .unwrap()
                                    .map(|v| web_sys::File::from(v.unwrap()))
                                    .map(File::from);
                                result.extend(files);
                            }
                            Msg::Files(result)
                        })}
                    />

                if let Some(chart) = &self.chart {
                    <div>
                        <p>{ "SyncTrack:" }</p>
                        <ul>
                        { for chart.sync_track.iter().map(|event| html!{ <li> { format!("{:?}", event) } </li> }) }
                        </ul>
                        <p>{ "Lyrics:" }</p>
                        <ul>
                        { for chart.lyrics.iter().map(|event| html!{ <li> { format!("{:?}", event) } </li> }) }
                        </ul>
                        <p>{ "Notes:" }</p>
                        <ul>
                        { for chart.key_presses.iter().map(|event| html!{ <li> { format!("{:?}", event) } </li> }) }
                        </ul>
                    </div>
                }
            </div>
        }
    }
}

fn main() {
    yew::start_app::<Model>();
}

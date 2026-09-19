//! Offline-only projection entry point. No BasicHost, provider, tool registry,
//! process spawning, config discovery, or network client is constructed here.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use xharness_session::{RequestHeader, SessionHeader, SessionStore};
use xharness_session_jsonl::JsonlSessionStore;

const INPUT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
struct Options {
    journal: Option<PathBuf>,
    output: PathBuf,
    rounds: usize,
    workers: usize,
    seconds: u64,
}
impl Options {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut result = Self {
            journal: None,
            output: PathBuf::new(),
            rounds: 100,
            workers: 1,
            seconds: 120,
        };
        let mut synthetic = false;
        let mut seen = std::collections::BTreeSet::new();
        let mut args = args.into_iter();
        while let Some(key) = args.next() {
            if !seen.insert(key.clone()) {
                return Err("duplicate option".into());
            }
            if key == "--synthetic" {
                synthetic = true;
                continue;
            }
            let value = args.next().ok_or("missing option value")?;
            match key.as_str() {
                "--journal" => result.journal = Some(value.into()),
                "--output" => result.output = value.into(),
                "--rounds" => result.rounds = value.parse().map_err(|_| "invalid rounds")?,
                "--workers" => result.workers = value.parse().map_err(|_| "invalid workers")?,
                "--seconds" => result.seconds = value.parse().map_err(|_| "invalid seconds")?,
                _ => return Err("unknown option".into()),
            }
        }
        if synthetic == result.journal.is_some() {
            return Err("select exactly one of --synthetic or --journal".into());
        }
        if !result.output.is_absolute()
            || !(1..=10000).contains(&result.rounds)
            || !(1..=4).contains(&result.workers)
            || !(1..=3600).contains(&result.seconds)
        {
            return Err("absolute new --output and bounded rounds/workers/seconds required".into());
        }
        Ok(result)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// Only a copy is passed to the store (whose recovery may truncate an incomplete
// tail). This function never opens the original with write permission.
fn stage(input: &Path, output: &Path) -> Result<(String, String), String> {
    let mut source = File::open(input).map_err(|_| "cannot open source journal")?;
    let before = source.metadata().map_err(|_| "cannot inspect source")?;
    if !before.is_file() || before.len() > INPUT_LIMIT {
        return Err("journal exceeds 64 MiB input budget or is not a file".into());
    }
    let staging = output.join("input.jsonl");
    let mut copy = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staging)
        .map_err(|_| "cannot create isolated copy")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = source.read(&mut buffer).map_err(|_| "source read failed")?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > INPUT_LIMIT {
            return Err("growing journal exceeds input budget".into());
        }
        hash.update(&buffer[..count]);
        copy.write_all(&buffer[..count])
            .map_err(|_| "copy write failed")?;
    }
    copy.sync_all().map_err(|_| "copy flush failed")?;
    drop(copy);
    let after = source.metadata().map_err(|_| "cannot recheck source")?;
    if total != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err("source changed while copying; use a stable snapshot".into());
    }
    let mut header = Vec::new();
    BufReader::new(File::open(&staging).map_err(|_| "cannot read copy")?)
        .take(1024 * 1024)
        .read_until(b'\n', &mut header)
        .map_err(|_| "cannot read journal header")?;
    let value: Value = serde_json::from_slice(&header).map_err(|_| "invalid journal header")?;
    let id = value
        .pointer("/header/id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or("invalid journal ID")?
        .to_owned();
    fs::create_dir(output.join("store")).map_err(|_| "cannot create isolated store")?;
    // Keep an immutable input snapshot separate from the store recovery copy.
    fs::copy(&staging, output.join("store").join(format!("{id}.jsonl")))
        .map_err(|_| "cannot stage store copy")?;
    Ok((id, format!("{:x}", hash.finalize())))
}

fn synthetic() -> Result<Session, String> {
    let mut session = Session::new(SessionHeader::new("projection-repro-synthetic"))
        .map_err(|_| "synthetic header failed")?;
    for turn in 1..=8 {
        let mut header = RequestHeader::new("offline", "offline");
        header.options.insert(
            "context".into(),
            json!({"edits":[{"type":"synthetic"}],"measurement":{"tokens":42}}),
        );
        let events = vec![
            EventData::TurnStart { turn }.into(),
            EventData::UserMessage {
                message: Message::user("synthetic question"),
                surface_replace: None,
            }
            .into(),
            EventData::StepStart { turn, step: 1 }.into(),
            EventData::RequestHeader { header }.into(),
            EventData::AssistantChunk {
                turn,
                step: 1,
                chunk: AssistantChunk::TextDelta("escaped \"text\"\n🧪".into()),
            }
            .into(),
            EventData::AssistantMessage {
                turn,
                step: 1,
                message: Message::assistant("synthetic answer"),
                usage: Some(json!({"input_tokens":128,"output_tokens":32})),
            }
            .into(),
            EventData::StepEnd { turn, step: 1 }.into(),
            EventData::TurnEnd {
                turn,
                reason: TurnEndReason::Completed,
            }
            .into(),
        ];
        session
            .append_batch(session.revision(), events)
            .map_err(|_| "synthetic event validation failed")?;
    }
    Ok(session)
}

fn verify(value: &Value) -> Result<usize, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "serialization failed")?;
    let count = serialized_json_size(value).map_err(|_| "byte counter failed")?;
    let decoded: Value = serde_json::from_slice(&bytes).map_err(|_| "round trip decode failed")?;
    if count != bytes.len() || &decoded != value {
        return Err("projection/count/round trip mismatch".into());
    }
    Ok(count)
}

fn checkpoint(file: &Mutex<File>, worker: usize, phase: &str, seq: u64) -> Result<(), String> {
    let mut file = file.lock().map_err(|_| "checkpoint lock poisoned")?;
    writeln!(
        file,
        "{{\"worker\":{worker},\"phase\":\"{phase}\",\"index\":{seq}}}"
    )
    .and_then(|_| file.flush())
    .map_err(|_| "checkpoint write failed".into())
}

pub async fn run_cli(args: Vec<String>) -> Result<(), String> {
    if args == ["--help"] {
        println!("--synthetic | --journal FILE; --output NEW_ABSOLUTE_DIR [--rounds 100] [--workers 1] [--seconds 120]");
        return Ok(());
    }
    let options = Options::parse(args)?;
    fs::create_dir(&options.output)
        .map_err(|_| "output must be a new directory with an existing parent")?;
    let progress = Arc::new(Mutex::new(
        File::create(options.output.join("progress.jsonl"))
            .map_err(|_| "cannot create checkpoint")?,
    ));
    checkpoint(&progress, 0, "load", 0)?;
    let (session, source_hash) = if let Some(input) = &options.journal {
        let (id, hash) = stage(input, &options.output)?;
        let store = JsonlSessionStore::new(options.output.join("store"))
            .map_err(|_| "isolated store open failed")?
            .for_runtime();
        let session = store
            .load(&id)
            .await
            .map_err(|_| "copy failed journal/lifecycle validation")?
            .ok_or("missing copied session")?;
        (session, Some(hash))
    } else {
        (synthetic()?, None)
    };
    if session.events().is_empty() {
        return Err("empty journal".into());
    }
    let manifest = json!({"format":"projection-repro-v1","sourceSha256":source_hash,"events":session.events().len(),"workers":options.workers,"rounds":options.rounds,"seconds":options.seconds,"models":false,"tools":false});
    fs::write(
        options.output.join("input-manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|_| "manifest encoding failed")?,
    )
    .map_err(|_| "manifest write failed")?;
    let deadline = Instant::now() + Duration::from_secs(options.seconds);
    let mut handles = Vec::new();
    for worker in 0..options.workers {
        let session = session.clone();
        let progress = Arc::clone(&progress);
        let rounds = options.rounds;
        handles.push(std::thread::spawn(
            move || -> Result<(usize, bool), String> {
                let route = ModelRoute::new("offline", "offline");
                let prompts = prompt_views(&session);
                let initial = initial_request_header_seq(&session);
                for event in session.events() {
                    if Instant::now() >= deadline {
                        return Ok((0, true));
                    }
                    checkpoint(&progress, worker, "construct_event", event.seq)?;
                    let value = restored_web_event(event, &route, &prompts, initial, None);
                    checkpoint(&progress, worker, "serialize_event", event.seq)?;
                    verify(&value)?;
                }
                for round in 0..rounds {
                    if Instant::now() >= deadline {
                        return Ok((round, true));
                    }
                    checkpoint(&progress, worker, "tail", round as u64)?;
                    let tail = project_session_event_tail(&session, &route, 64, 256 * 1024);
                    for event in &tail.events {
                        verify(event)?;
                    }
                    // Vary historical boundaries while retaining original journal semantics.
                    let before = ((round * 97 + worker) % session.events().len()) as u64;
                    checkpoint(&progress, worker, "history", before)?;
                    let page = project_session_history(&session, &route, Some(before), 20);
                    for event in &page.events {
                        verify(event)?;
                    }
                }
                Ok((rounds, false))
            },
        ));
    }
    let mut completed = 0;
    let mut timed_out = false;
    let mut failed = false;
    for handle in handles {
        match handle.join() {
            Ok(Ok((rounds, timeout))) => {
                completed += rounds;
                timed_out |= timeout;
            }
            _ => failed = true,
        }
    }
    let result = json!({"format":"projection-repro-result-v1","completedRounds":completed,"timedOut":timed_out,"failed":failed,"crashReproduced":false});
    fs::write(
        options.output.join("result.json"),
        serde_json::to_vec_pretty(&result).map_err(|_| "result encoding failed")?,
    )
    .map_err(|_| "result write failed")?;
    if failed {
        return Err("projection worker failed; see last checkpoint".into());
    }
    if timed_out {
        return Err("workload deadline reached; incomplete run".into());
    }
    println!("projection-repro: completed {completed} rounds; no models/tools; no native crash reproduced");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_ids_and_unbounded_options() {
        for id in ["", "../escape", "C:\\outside", "x/y", "."] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("session-123_4"));
        for args in [
            vec!["--synthetic", "--output", "relative"],
            vec!["--synthetic", "--workers", "9"],
            vec!["--synthetic", "--journal", "x"],
        ] {
            assert!(Options::parse(args.into_iter().map(str::to_owned).collect()).is_err());
        }
    }
    #[test]
    fn synthetic_exercises_real_projection_and_counting() {
        let session = synthetic().unwrap();
        let route = ModelRoute::new("offline", "offline");
        let prompts = prompt_views(&session);
        for event in session.events() {
            verify(&restored_web_event(
                event,
                &route,
                &prompts,
                initial_request_header_seq(&session),
                None,
            ))
            .unwrap();
        }
    }
}

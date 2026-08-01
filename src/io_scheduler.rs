use crate::cancel::CancellationToken;
use crate::errors::FileError;
use crate::types::IoProfile;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::thread;

pub const NUM_IO_THREADS_DEFAULT: usize = 3;
const JOB_CAP: usize = 512;
const BUF_CAP: usize = 256;

#[derive(Debug)]
pub struct IOJob {
    pub path: PathBuf,
    pub size: u64,
    pub offset: u64,
    pub length: usize,
}

#[derive(Debug)]
pub struct IOResult {
    pub path: PathBuf,
    pub offset: u64,
    pub data: Vec<u8>,
    pub error: Option<FileError>,
}

fn read_job(job: &IOJob) -> IOResult {
    let mut data = vec![0u8; job.length];
    let mut file = match File::open(&job.path) {
        Ok(f) => f,
        Err(e) => {
            return IOResult {
                path: job.path.clone(),
                offset: job.offset,
                data: Vec::new(),
                error: Some(FileError::Io(e.to_string())),
            };
        }
    };
    if job.offset > 0 {
        if let Err(e) = file.seek(SeekFrom::Start(job.offset)) {
            return IOResult {
                path: job.path.clone(),
                offset: job.offset,
                data: Vec::new(),
                error: Some(FileError::Io(e.to_string())),
            };
        }
    }
    let mut read_total = 0usize;
    while read_total < job.length {
        match file.read(&mut data[read_total..]) {
            Ok(0) => break,
            Ok(n) => read_total += n,
            Err(e) => {
                return IOResult {
                    path: job.path.clone(),
                    offset: job.offset,
                    data: Vec::new(),
                    error: Some(FileError::Io(e.to_string())),
                };
            }
        }
    }
    data.truncate(read_total);
    IOResult {
        path: job.path.clone(),
        offset: job.offset,
        data,
        error: None,
    }
}

fn io_thread_count(profile: IoProfile, network_heavy: bool) -> usize {
    match profile {
        IoProfile::HddSafe => 2,
        IoProfile::SsdFast => 4,
        IoProfile::Auto => {
            if network_heavy {
                2
            } else {
                NUM_IO_THREADS_DEFAULT
            }
        }
    }
}

/// Sort jobs by parent directory for locality, then process with a small reader pool.
pub fn run_io_jobs(
    mut jobs: Vec<IOJob>,
    token: &CancellationToken,
    profile: IoProfile,
    network_heavy: bool,
) -> Vec<IOResult> {
    jobs.sort_by_key(|j| j.path.parent().map(|p| p.to_path_buf()));
    let n = io_thread_count(profile, network_heavy).max(1);
    let (job_tx, job_rx) = bounded::<Option<IOJob>>(JOB_CAP);
    let (out_tx, out_rx) = bounded::<IOResult>(BUF_CAP);

    for j in jobs {
        if token.is_cancelled() {
            break;
        }
        if job_tx.send(Some(j)).is_err() {
            break;
        }
    }
    for _ in 0..n {
        let _ = job_tx.send(None);
    }

    let mut handles = Vec::new();
    for _ in 0..n {
        let jr = job_rx.clone();
        let ot = out_tx.clone();
        let token = token.clone_token();
        handles.push(thread::spawn(move || {
            while let Ok(Some(job)) = jr.recv() {
                if token.is_cancelled() {
                    break;
                }
                let res = read_job(&job);
                let _ = ot.send(res);
            }
        }));
    }
    drop(out_tx);

    let mut results = Vec::new();
    while let Ok(res) = out_rx.recv() {
        if token.is_cancelled() {
            break;
        }
        results.push(res);
    }
    for h in handles {
        let _ = h.join();
    }
    results
}

/// Build three read jobs per file for partial hashing regions.
pub fn partial_jobs(path: PathBuf, size: u64, chunk_size: usize) -> Vec<IOJob> {
    let chunk = chunk_size as u64;
    if size == 0 {
        return vec![IOJob {
            path,
            size,
            offset: 0,
            length: 0,
        }];
    }
    let mid = (size / 2).saturating_sub(chunk / 2);
    let end = size.saturating_sub(chunk);
    vec![
        IOJob {
            path: path.clone(),
            size,
            offset: 0,
            length: chunk_size,
        },
        IOJob {
            path: path.clone(),
            size,
            offset: mid,
            length: chunk_size,
        },
        IOJob {
            path,
            size,
            offset: end,
            length: chunk_size,
        },
    ]
}

pub type JobSender = Sender<Option<IOJob>>;
pub type JobReceiver = Receiver<Option<IOJob>>;
pub type OutSender = Sender<IOResult>;
pub type OutReceiver = Receiver<IOResult>;

pub fn io_channels() -> (JobSender, JobReceiver, OutSender, OutReceiver) {
    let (jtx, jrx) = bounded(JOB_CAP);
    let (otx, orx) = bounded(BUF_CAP);
    (jtx, jrx, otx, orx)
}

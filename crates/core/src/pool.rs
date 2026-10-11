//! Thread pools for `RenderOptions::threads`. `None` uses rayon's global
//! pool and `Some(1)` the calling thread (no pool); any other size gets its
//! own pool, built once per size and kept for the life of the process
//! (pools are expensive to start).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::InkoxideError;

/// Upper bound on a requested pool size.
const MAX_THREADS: usize = 256;

static POOLS: Mutex<Option<HashMap<usize, Arc<ThreadPool>>>> = Mutex::new(None);

pub(crate) fn get(threads: usize) -> Result<Arc<ThreadPool>, InkoxideError> {
    let n = threads.clamp(1, MAX_THREADS);
    let mut pools = POOLS
        .lock()
        .map_err(|_| InkoxideError::Internal("thread pool registry poisoned".into()))?;
    let pools = pools.get_or_insert_with(HashMap::new);
    if let Some(p) = pools.get(&n) {
        return Ok(p.clone());
    }
    let pool = ThreadPoolBuilder::new()
        .num_threads(n)
        .thread_name(|i| format!("inkoxide-{i}"))
        .build()
        .map_err(|e| InkoxideError::Internal(format!("thread pool: {e}")))?;
    let pool = Arc::new(pool);
    pools.insert(n, pool.clone());
    Ok(pool)
}

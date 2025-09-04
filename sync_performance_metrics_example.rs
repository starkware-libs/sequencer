// Example: Adding metrics to investigate slow sync performance
// Add these to crates/apollo_state_sync_metrics/src/metrics.rs

define_metrics!(
    StateSync => {
        // Existing metrics...

        // NEW METRICS FOR SYNC PERFORMANCE INVESTIGATION:

        // 1. Track sync lag (how far behind we are)
        MetricGauge {
            SYNC_LAG_BLOCKS,
            "apollo_sync_lag_blocks",
            "Number of blocks we're behind the network"
        },

        // 2. Measure block download time
        MetricHistogram {
            BLOCK_DOWNLOAD_DURATION_SECS,
            "apollo_block_download_duration_seconds",
            "Time taken to download individual blocks from network"
        },

        // 3. Measure block processing time
        MetricHistogram {
            BLOCK_PROCESSING_DURATION_SECS,
            "apollo_block_processing_duration_seconds",
            "Time taken to process and store a block locally"
        },

        // 4. Track network fetch failures
        MetricCounter {
            NETWORK_FETCH_FAILURES,
            "apollo_network_fetch_failures_total",
            "Number of failed attempts to fetch data from network",
            init = 0
        },

        // 5. Track batch sizes
        MetricHistogram {
            SYNC_BATCH_SIZE,
            "apollo_sync_batch_size",
            "Number of blocks downloaded in each batch"
        },

        // 6. Measure storage flush time (the "flushing mmap files" we saw)
        MetricHistogram {
            STORAGE_FLUSH_DURATION_SECS,
            "apollo_storage_flush_duration_seconds",
            "Time taken to flush storage to disk"
        },
    },
);

// Usage examples in sync code:

// 1. In stream_new_blocks function:
fn stream_new_blocks() {
    // Before downloading
    let download_start = std::time::Instant::now();

    // Download blocks...
    let blocks = central_source.stream_new_blocks(start, end).await?;

    // Record download time
    BLOCK_DOWNLOAD_DURATION_SECS.record(download_start.elapsed().as_secs_f64());

    // Record batch size
    SYNC_BATCH_SIZE.record((end.0 - start.0) as f64);
}

// 2. In process_sync_event function:
async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
    let processing_start = std::time::Instant::now();

    // Process the event...
    let result = match sync_event {
        SyncEvent::BlockAvailable { block_number, block, signature } => {
            // Process block...
        }
    };

    // Record processing time
    BLOCK_PROCESSING_DURATION_SECS.record(processing_start.elapsed().as_secs_f64());

    result
}

// 3. In storage flush function (apollo_storage/src/lib.rs):
fn flush(&self) {
    let flush_start = std::time::Instant::now();
    debug!("Flushing the mmap files.");

    // Do the actual flushing...
    self.thin_state_diff.flush();
    self.contract_class.flush();
    // ... other flushes

    // Record flush time
    STORAGE_FLUSH_DURATION_SECS.record(flush_start.elapsed().as_secs_f64());
}

// 4. Calculate sync lag:
fn update_sync_lag(&self) {
    let latest_network_block = self.get_latest_network_block().await?;
    let local_block = self.get_local_block_height()?;
    let lag = latest_network_block.0 - local_block.0;

    SYNC_LAG_BLOCKS.set(lag as f64);
}

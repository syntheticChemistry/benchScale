// SPDX-License-Identifier: AGPL-3.0-only
//! Timeout and Retry Utilities
//!
//! Pure, testable functions for timeout and retry logic that don't depend on
//! external state or connections. These can be tested in isolation without
//! requiring libvirt, Docker, or any other infrastructure.

use crate::{Error, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;
use tracing::debug;

/// Exponential backoff configuration
///
/// Controls retry behavior with exponential delays between attempts.
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Maximum number of attempts (including initial)
    pub max_attempts: usize,
    /// Multiplier for exponential growth
    pub multiplier: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            max_attempts: 20,
            multiplier: 1.5,
        }
    }
}

impl BackoffConfig {
    /// Create config for quick retries (testing)
    pub fn quick() -> Self {
        Self {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            max_attempts: 5,
            multiplier: 2.0,
        }
    }

    /// Create config for patient retries (production)
    pub fn patient() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_attempts: 30,
            multiplier: 1.5,
        }
    }
}

/// Retry a fallible async operation with exponential backoff
///
/// This is a pure, testable function that implements retry logic with
/// exponential backoff. It doesn't depend on any external state and can
/// be used for any async operation (SSH, HTTP, database, etc.).
///
/// # Type Parameters
/// - `F`: Function that creates the future
/// - `Fut`: Future type returned by the function
/// - `T`: Success type
/// - `E`: Error type (must implement Display)
///
/// # Example
/// ```no_run
/// use benchscale::backend::timeout_utils::{retry_with_backoff, BackoffConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// let result = retry_with_backoff(
///     || async {
///         // Your operation here
///         Ok::<_, String>(42)
///     },
///     BackoffConfig::default(),
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    config: BackoffConfig,
) -> std::result::Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    // Deep debt solution: Handle edge case instead of unreachable!()
    if config.max_attempts == 0 {
        return operation().await;
    }

    let mut delay = config.initial_delay;

    for attempt in 1..=config.max_attempts {
        debug!("Attempt {}/{}", attempt, config.max_attempts);

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("Operation succeeded after {} attempts", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                debug!("Attempt {} failed: {}", attempt, e);

                // Return error on last attempt
                if attempt == config.max_attempts {
                    return Err(e);
                }

                debug!("Waiting {:?} before retry", delay);
                tokio::time::sleep(delay).await;

                // Exponential backoff with max cap
                delay = std::cmp::min(
                    Duration::from_secs_f64(delay.as_secs_f64() * config.multiplier),
                    config.max_delay,
                );
            }
        }
    }

    // Deep debt: This is truly unreachable now (loop always returns)
    // But Rust can't prove it, so we satisfy the compiler with a panic
    // that explains the logic error if it ever happens.
    panic!(
        "BUG: retry_with_backoff loop didn't return. Attempts: {}. This should never happen.",
        config.max_attempts
    )
}

/// Wait for a condition to become true, with timeout
///
/// Polls a condition function repeatedly until it returns true or timeout is reached.
/// This is a pure function that doesn't depend on any external state.
///
/// # Arguments
/// - `check`: Async function that returns true when condition is met
/// - `timeout`: Maximum time to wait
/// - `poll_interval`: How often to check the condition
///
/// # Returns
/// - `Ok(())` if condition becomes true before timeout
/// - `Err(...)` if timeout is reached
///
/// # Example
/// ```no_run
/// use benchscale::backend::timeout_utils::wait_for_condition;
/// use std::time::Duration;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut ready = false;
///
/// wait_for_condition(
///     || async { ready },
///     Duration::from_secs(30),
///     Duration::from_millis(100),
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn wait_for_condition<F, Fut>(
    mut check: F,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = Instant::now();
    let mut attempt = 0;

    loop {
        attempt += 1;

        if check().await {
            debug!(
                "Condition met after {} attempts ({:?})",
                attempt,
                start.elapsed()
            );
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(Error::Backend(format!(
                "Condition not met after {:?} ({} attempts)",
                timeout, attempt
            )));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Wait for a condition with exponential backoff between checks
///
/// Similar to `wait_for_condition` but uses exponential backoff between polls.
/// Useful when checking is expensive or has side effects.
///
/// # Example
/// ```no_run
/// use benchscale::backend::timeout_utils::{wait_for_condition_backoff, BackoffConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// wait_for_condition_backoff(
///     || async { /* check something */ true },
///     BackoffConfig::default(),
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn wait_for_condition_backoff<F, Fut>(mut check: F, config: BackoffConfig) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    // Deep debt solution: Handle edge case instead of unreachable!()
    if config.max_attempts == 0 {
        return if check().await {
            Ok(())
        } else {
            Err(Error::Backend(
                "Condition not met (0 attempts configured)".to_string(),
            ))
        };
    }

    let mut delay = config.initial_delay;

    for attempt in 1..=config.max_attempts {
        debug!(
            "Checking condition, attempt {}/{}",
            attempt, config.max_attempts
        );

        if check().await {
            debug!("Condition met after {} attempts", attempt);
            return Ok(());
        }

        if attempt == config.max_attempts {
            return Err(Error::Backend(format!(
                "Condition not met after {} attempts",
                config.max_attempts
            )));
        }

        debug!("Condition not met, waiting {:?}", delay);
        tokio::time::sleep(delay).await;

        delay = std::cmp::min(
            Duration::from_secs_f64(delay.as_secs_f64() * config.multiplier),
            config.max_delay,
        );
    }

    // Deep debt: This is truly unreachable now (loop always returns)
    // But satisfy the compiler with a descriptive panic for any logic bugs
    panic!(
        "BUG: wait_for_condition_backoff loop didn't return. Attempts: {}. This should never happen.",
        config.max_attempts
    )
}

// ============================================================================
// UNIT TESTS (No external dependencies needed!)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_retry_with_backoff_success_first_try() {
        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(
            || {
                let attempts = attempts_clone.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Ok::<_, String>(42)
                }
            },
            BackoffConfig::quick(),
        )
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(*attempts.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_after_retries() {
        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(
            || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = {
                        let mut a = attempts.lock().unwrap();
                        *a += 1;
                        *a
                    };
                    if count < 3 {
                        Err("not yet")
                    } else {
                        Ok(42)
                    }
                }
            },
            BackoffConfig::quick(),
        )
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(*attempts.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_exhaustion() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            max_attempts: 3,
            multiplier: 2.0,
        };

        let attempts = Arc::new(Mutex::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(
            || {
                let attempts = attempts_clone.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Err::<(), _>("always fails")
                }
            },
            config,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(*attempts.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_wait_for_condition_success() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = wait_for_condition(
            || {
                let counter = counter_clone.clone();
                async move {
                    let mut c = counter.lock().unwrap();
                    *c += 1;
                    *c >= 3
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
        assert!(*counter.lock().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_wait_for_condition_timeout() {
        let result = wait_for_condition(
            || async { false }, // Never succeeds
            Duration::from_millis(100),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Condition not met"));
    }

    #[tokio::test]
    async fn test_wait_for_condition_immediate_success() {
        let result = wait_for_condition(
            || async { true }, // Already true
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_condition_backoff_success() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = wait_for_condition_backoff(
            || {
                let counter = counter_clone.clone();
                async move {
                    let mut c = counter.lock().unwrap();
                    *c += 1;
                    *c >= 3
                }
            },
            BackoffConfig::quick(),
        )
        .await;

        assert!(result.is_ok());
        assert!(*counter.lock().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_wait_for_condition_backoff_exhaustion() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            max_attempts: 3,
            multiplier: 2.0,
        };

        let result = wait_for_condition_backoff(|| async { false }, config).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Condition not met"));
    }

    #[tokio::test]
    async fn test_backoff_config_quick() {
        let config = BackoffConfig::quick();
        assert_eq!(config.initial_delay, Duration::from_millis(10));
        assert_eq!(config.max_attempts, 5);
    }

    #[tokio::test]
    async fn test_backoff_config_patient() {
        let config = BackoffConfig::patient();
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_attempts, 30);
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            max_attempts: 5,
            multiplier: 2.0,
        };

        let start = Instant::now();

        let _result = retry_with_backoff(|| async { Err::<(), _>("fail") }, config).await;

        let elapsed = start.elapsed();

        // Should have delays: 10ms + 20ms + 40ms + 80ms = 150ms minimum
        assert!(
            elapsed >= Duration::from_millis(150),
            "Backoff too fast: {:?}",
            elapsed
        );
    }
}

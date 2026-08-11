//! Declaring to the core what **keestore itself** stores, per account.
//!
//! ## The attribution rule
//!
//! Whoever physically holds the byte declares it, and only them. Keestore holds
//! all of its own: one KeePass database per account, written to its
//! `StorageBackend` at `vaults.kdbx_path`, with the size the write path recorded
//! in `vaults.file_size_bytes`. Nothing goes through drive, so keestore never
//! emits `delegated`.
//!
//! ## One category, and why it is billed
//!
//! The criterion is the platform's: **an account is billed for what it can free
//! itself**. A vault is created by its owner, it is theirs alone
//! (`vaults.owner_id` is `UNIQUE`), and deleting it frees exactly those bytes —
//! so it is `content`, billed, with nothing else to declare. Keestore keeps no
//! thumbnail, no index and no cache: there is nothing about an encrypted
//! password database it could usefully derive.
//!
//! ## What this reporter never touches
//!
//! The vault is end-to-end encrypted and this reporter does not open it. It reads
//! one integer column per account and sends a number. It carries no entry count,
//! no title, no hash — an administrator learns that an account holds a vault of
//! *n* bytes, which is the whole point of a storage breakdown and the entirety of
//! what it is allowed to learn.
//!
//! `file_size_bytes` rather than a `stat` on `kdbx_path`: the module runs under a
//! seccomp policy, the column is written by the same path that writes the file,
//! and reading the filesystem to confirm what the database already says would
//! trade a guarantee for a syscall.
//!
//! ## State, never deltas
//!
//! Each declaration carries keestore's **current** figure for every account, so
//! re-sending one changes nothing and a message lost in flight costs one stale
//! number until the next declaration repairs it. The core keys rows on
//! `(module_id, user_id, category)`; idempotence is structural.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::state::AppState;

/// How often the complete state is recounted and declared. Same period as the
/// other modules, so a console refresh does not show one of them systematically
/// staler than the rest.
const FULL_SYNC_INTERVAL: Duration = Duration::from_secs(6 * 3_600);

/// First retry delay when a declaration could not be delivered, doubling up to
/// [`FULL_SYNC_INTERVAL`].
///
/// The module starts before the core has necessarily finished accepting
/// registrations, so the very first declaration routinely fails. Without a
/// backoff it would be re-attempted six hours later and the breakdown would sit
/// empty for an afternoon after every reboot.
const FULL_RETRY_MIN: Duration = Duration::from_secs(15);

/// Matches the core's own per-request ceiling (`storage::usage::MAX_ENTRIES`).
const MAX_ENTRIES: usize = 5_000;

/// Identifier this module declares under. Only consulted by the core when the
/// caller could not be identified from its `X-Internal-Secret`: the core prefers
/// the secret's identity and answers 403 when the two disagree, so naming
/// ourselves in the body can never impersonate another module.
const MODULE_ID: &str = "keestore";

/// What the account created and can delete. Billed.
const CAT_CONTENT: &str = "content";

/// Every byte-bearing query keestore runs, paired with the category it feeds.
///
/// Each statement must return exactly `(owner uuid, bytes bigint, objects bigint)`
/// and must only read keestore's own schema. A single entry today; the table
/// exists so that adding a second one cannot quietly skip the guard tests below.
///
/// The `GROUP BY` is not redundant with `owner_id`'s `UNIQUE` constraint: it is
/// what makes the query keep returning one row per account if that constraint is
/// ever relaxed to allow several vaults.
const OWNED_QUERIES: &[(&str, &str)] = &[(
    CAT_CONTENT,
    "SELECT owner_id,
            COALESCE(SUM(file_size_bytes), 0)::bigint,
            COUNT(*)::bigint
       FROM keestore.vaults
      GROUP BY owner_id",
)];

/// One `(account, category)` figure, as declared.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    user_id: Uuid,
    category: &'static str,
    used_bytes: i64,
    object_count: i64,
}

/// Recounts everything keestore holds.
///
/// A failing query is logged and skipped rather than aborting the whole
/// declaration. The declaration is still marked `full`, which means a category
/// that failed here is *retired* by the core until the next sync repairs it — the
/// honest outcome, since publishing a stale figure as current state would be
/// worse than publishing none.
async fn collect(db: &PgPool) -> Vec<Entry> {
    let mut acc: HashMap<(Uuid, &'static str), (i64, i64)> = HashMap::new();

    for (category, sql) in OWNED_QUERIES {
        match sqlx::query_as::<_, (Uuid, i64, i64)>(sql).fetch_all(db).await {
            Ok(rows) => {
                for (user_id, bytes, objects) in rows {
                    let slot = acc.entry((user_id, *category)).or_insert((0, 0));
                    slot.0 += bytes;
                    slot.1 += objects;
                }
            }
            Err(e) => tracing::error!(
                error = %e,
                catégorie = *category,
                "Recomptage de consommation échoué pour une requête — catégorie incomplète"
            ),
        }
    }

    let mut entries: Vec<Entry> = acc
        .into_iter()
        .map(|((user_id, category), (used_bytes, object_count))| Entry {
            user_id,
            category,
            used_bytes,
            object_count,
        })
        .collect();

    // Stable order so consecutive declarations chunk identically — a moving
    // chunk boundary would make partial declarations retire different accounts
    // each time.
    entries.sort_by(|a, b| (a.user_id, a.category).cmp(&(b.user_id, b.category)));
    entries
}

/// How many calls a declaration of `n` entries takes. Zero entries still takes
/// one: an empty `full` declaration is a statement, not a no-op.
fn page_count(n: usize) -> usize {
    if n == 0 { 1 } else { n.div_ceil(MAX_ENTRIES) }
}

/// Whether `full` may actually be claimed on the wire.
///
/// A chunked declaration marked `full` would retire every entry outside whichever
/// chunk happened to be sent last.
fn claims_full(full: bool, n: usize) -> bool {
    full && n <= MAX_ENTRIES
}

/// Sends one declaration to the core.
async fn send(
    http: &reqwest::Client,
    state: &AppState,
    entries: &[Entry],
    full: bool,
) -> Result<(), String> {
    let url = format!("{}/internal/storage/usage", state.settings.core.url);
    let usage: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "user_id":      e.user_id,
                "category":     e.category,
                "used_bytes":   e.used_bytes,
                "object_count": e.object_count,
            })
        })
        .collect();

    let resp = http
        .post(&url)
        .header(
            "X-Internal-Secret",
            state.settings.core.internal_secret.as_str(),
        )
        .json(&json!({ "module_id": MODULE_ID, "full": full, "usage": usage }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        return Ok(());
    }

    // The status alone does not say which of several validations refused the
    // declaration, and this runs unattended — a log line reading "HTTP 422" costs
    // an afternoon the next time the contract shifts.
    let status = resp.status();
    let detail = resp.text().await.unwrap_or_default();
    let detail: String = detail.chars().take(300).collect();
    Err(format!("HTTP {status} {detail}"))
}

/// Declares `entries` in as many calls as the core's ceiling requires.
///
/// Returns `true` when every call landed; the caller reschedules on that.
async fn declare(http: &reqwest::Client, state: &AppState, entries: Vec<Entry>, full: bool) -> bool {
    let full_on_wire = claims_full(full, entries.len());
    if full && !full_on_wire {
        tracing::warn!(
            entrées = entries.len(),
            envois = page_count(entries.len()),
            "Synchronisation complète découpée : déclarée en plusieurs envois partiels"
        );
    }

    // An empty full declaration is meaningful and must still be sent: it is how
    // keestore says "I hold nothing for anybody", which the core has to be able
    // to tell apart from "keestore has never declared".
    if entries.is_empty() {
        if !full_on_wire {
            return true;
        }
        return match send(http, state, &[], true).await {
            Ok(()) => {
                tracing::debug!("Consommation déclarée : aucune entrée");
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "Déclaration de consommation échouée");
                false
            }
        };
    }

    let mut declared_bytes: i64 = 0;
    let mut sent = 0usize;
    let mut all_ok = true;
    for chunk in entries.chunks(MAX_ENTRIES) {
        match send(http, state, chunk, full_on_wire).await {
            Ok(()) => {
                declared_bytes += chunk.iter().map(|e| e.used_bytes).sum::<i64>();
                sent += chunk.len();
            }
            Err(e) => {
                all_ok = false;
                tracing::warn!(error = %e, entrées = chunk.len(), "Déclaration de consommation échouée");
            }
        }
    }

    if sent > 0 {
        tracing::debug!(
            entrées = sent,
            octets = declared_bytes,
            complète = full_on_wire,
            "Consommation déclarée au core"
        );
    }
    all_ok
}

/// The reporter task. Started once at bootstrap.
pub async fn run_reporter(state: AppState) {
    let http = reqwest::Client::new();

    // Absolute deadline rather than an `interval`, so a failed sync can be pulled
    // forward without the retries drifting the normal period.
    let mut next_at = tokio::time::Instant::now(); // the first one is immediate
    let mut backoff = FULL_RETRY_MIN;

    tracing::info!("Rapporteur de consommation démarré (déclaration au core)");

    loop {
        tokio::time::sleep_until(next_at).await;

        let entries = collect(&state.db).await;
        let delivered = declare(&http, &state, entries, true).await;

        let now = tokio::time::Instant::now();
        if delivered {
            next_at = now + FULL_SYNC_INTERVAL;
            backoff = FULL_RETRY_MIN;
        } else {
            next_at = now + backoff;
            backoff = (backoff * 2).min(FULL_SYNC_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault is its owner's and nobody else's. Anything that grouped by another
    /// column would either bill the wrong person or bill several.
    #[test]
    fn every_query_groups_by_the_vault_owner() {
        for (_, sql) in OWNED_QUERIES {
            assert!(
                sql.to_lowercase().contains("group by owner_id"),
                "requête sans GROUP BY owner_id — une ligne par compte est le contrat : {sql}"
            );
        }
    }

    /// Keestore writes nothing into drive, so it must never claim a delegation,
    /// and it holds nothing derivable, so it must declare nothing but content.
    #[test]
    fn content_is_the_only_category() {
        use std::collections::BTreeSet;
        let cats: BTreeSet<&str> = OWNED_QUERIES.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            cats,
            BTreeSet::from([CAT_CONTENT]),
            "keestore ne détient qu'un coffre par compte : rien d'autre à déclarer"
        );
    }

    /// A module reading another module's schema would be both an architecture
    /// violation and a double count waiting to happen.
    #[test]
    fn queries_only_read_the_keestore_schema() {
        for (_, sql) in OWNED_QUERIES {
            let lowered = sql.to_lowercase();
            for foreign in ["drive.", "core.", "chat.", "office.", "mail."] {
                assert!(
                    !lowered.contains(foreign),
                    "la requête lit le schéma « {foreign} » : {sql}"
                );
            }
            assert!(
                lowered.contains("keestore."),
                "la requête ne lit aucune table de keestore : {sql}"
            );
        }
    }

    /// The vault is encrypted and stays that way: a declaration carries a size,
    /// never anything that describes what is inside.
    #[test]
    fn nothing_about_the_vaults_contents_is_read() {
        for (_, sql) in OWNED_QUERIES {
            let lowered = sql.to_lowercase();
            for private in ["kdbx_path", "file_hash_sha256", "unlock_attempts"] {
                assert!(
                    !lowered.contains(private),
                    "la requête lit « {private} » — une déclaration ne porte qu'une taille : {sql}"
                );
            }
        }
    }

    #[test]
    fn paging_respects_the_core_ceiling() {
        assert_eq!(page_count(0), 1, "une déclaration vide reste une déclaration");
        assert_eq!(page_count(MAX_ENTRIES), 1);
        assert_eq!(page_count(MAX_ENTRIES + 1), 2);
    }

    #[test]
    fn full_is_only_claimed_when_it_fits_in_one_call() {
        assert!(claims_full(true, MAX_ENTRIES));
        assert!(
            !claims_full(true, MAX_ENTRIES + 1),
            "une déclaration découpée ne peut pas se dire complète"
        );
        assert!(!claims_full(false, 1));
    }

    #[test]
    fn chunking_covers_every_entry_exactly_once() {
        let entries: Vec<Entry> = (0..MAX_ENTRIES + 3)
            .map(|i| Entry {
                user_id: Uuid::from_u128(i as u128),
                category: CAT_CONTENT,
                used_bytes: 1,
                object_count: 1,
            })
            .collect();
        let seen: usize = entries.chunks(MAX_ENTRIES).map(<[Entry]>::len).sum();
        assert_eq!(seen, entries.len());
    }
}

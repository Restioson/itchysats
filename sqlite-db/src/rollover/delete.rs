use crate::models::OrderId;
use anyhow::Context;
use anyhow::Result;
use sqlx::Sqlite;
use sqlx::Transaction;

pub(crate) async fn delete(
    inner_transaction: &mut Transaction<'_, Sqlite>,
    offer_id: OrderId,
) -> Result<()> {
    sqlx::query!(
        r#"
            delete from rollover_completed_event_data where cfd_id = (select id from cfds where cfds.uuid = $1)
        "#,
        offer_id
    )
        .execute(&mut *inner_transaction)
        .await
        .with_context(|| format!("Failed to delete from rollover_completed_event_data for {offer_id}"))?;

    sqlx::query!(
        r#"
            delete from revoked_commit_transactions where cfd_id = (select id from cfds where cfds.uuid = $1)
        "#,
        offer_id
    )
        .execute(&mut *inner_transaction)
        .await
        .with_context(|| format!("Failed to delete from revoked_commit_transactions for {offer_id}"))?;

    sqlx::query!(
        r#"
            delete from open_cets where cfd_id = (select id from cfds where cfds.uuid = $1)
        "#,
        offer_id
    )
    .execute(&mut *inner_transaction)
    .await
    .with_context(|| format!("Failed to delete from open_cets for {offer_id}"))?;

    Ok(())
}

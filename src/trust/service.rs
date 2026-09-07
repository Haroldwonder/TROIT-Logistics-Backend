use crate::{
    errors::AppError,
    models::seller::{SellerGrade, SellerProfile, SellerTrustHistory, SellerTrustLevel},
    trust::models::TrustEvaluationResult,
};
use sqlx::{query_as, PgPool};
use tracing::info;
use uuid::Uuid;

/// Deterministically evaluates Seller Trust Level (LV1 to LV5)
pub fn calculate_trust_level(successful_txs: i32, fulfillment_rate: f64) -> SellerTrustLevel {
    if successful_txs >= 50 && fulfillment_rate >= 98.0 {
        SellerTrustLevel::LV5
    } else if successful_txs >= 30 && fulfillment_rate >= 95.0 {
        SellerTrustLevel::LV4
    } else if successful_txs >= 15 && fulfillment_rate >= 93.0 {
        SellerTrustLevel::LV3
    } else if successful_txs >= 5 && fulfillment_rate >= 90.0 {
        SellerTrustLevel::LV2
    } else {
        SellerTrustLevel::LV1
    }
}

/// Deterministically evaluates Seller Grade (Grade C, Grade B, Grade A)
/// IMPORTANT: Represents supplier trust built with Troit, NOT product physical condition.
pub fn calculate_seller_grade(successful_txs: i32, fulfillment_rate: f64) -> SellerGrade {
    if successful_txs >= 25 && fulfillment_rate >= 95.0 {
        SellerGrade::GradeA
    } else if successful_txs >= 10 {
        SellerGrade::GradeB
    } else {
        SellerGrade::GradeC
    }
}

pub fn trust_level_to_str(level: SellerTrustLevel) -> &'static str {
    match level {
        SellerTrustLevel::LV1 => "LV1",
        SellerTrustLevel::LV2 => "LV2",
        SellerTrustLevel::LV3 => "LV3",
        SellerTrustLevel::LV4 => "LV4",
        SellerTrustLevel::LV5 => "LV5",
    }
}

pub fn seller_grade_to_str(grade: SellerGrade) -> &'static str {
    match grade {
        SellerGrade::GradeC => "Grade C",
        SellerGrade::GradeB => "Grade B",
        SellerGrade::GradeA => "Grade A",
    }
}

/// Records a completed transaction for a seller and updates trust metrics transactionally.
pub async fn record_successful_transaction(
    db: &PgPool,
    user_id: Uuid,
    order_id: Uuid,
) -> Result<TrustEvaluationResult, AppError> {
    // 1. Ensure seller_profile exists, or create default
    let profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        INSERT INTO seller_profiles (user_id, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status)
        VALUES ($1, 'LV1', 'Grade C', 0, 100.0, 'PENDING')
        ON CONFLICT (user_id) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let old_level = profile.trust_level.clone();
    let old_grade = profile.seller_grade.clone();

    // 2. Increment successful transaction count
    let new_successful_txs = profile.successful_transactions + 1;
    let new_fulfillment_rate = 100.0; // In MVP baseline, fulfillment rate remains 100% on completed orders

    // 3. Evaluate new Trust Level and Seller Grade
    let new_trust_enum = calculate_trust_level(new_successful_txs, new_fulfillment_rate);
    let new_grade_enum = calculate_seller_grade(new_successful_txs, new_fulfillment_rate);

    let new_level_str = trust_level_to_str(new_trust_enum);
    let new_grade_str = seller_grade_to_str(new_grade_enum);

    let level_changed = old_level != new_level_str;

    // 4. Update seller_profiles in database
    let updated_profile: SellerProfile = query_as::<_, SellerProfile>(
        r#"
        UPDATE seller_profiles
        SET successful_transactions = $1,
            fulfillment_rate = $2,
            trust_level = $3,
            seller_grade = $4,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $5
        RETURNING id, user_id, store_name, store_address, trust_level, seller_grade, successful_transactions, fulfillment_rate, verification_status, created_at, updated_at
        "#
    )
    .bind(new_successful_txs)
    .bind(new_fulfillment_rate)
    .bind(new_level_str)
    .bind(new_grade_str)
    .bind(profile.id)
    .fetch_one(db)
    .await?;

    // 5. Append audit log to seller_trust_history if trust level changed
    if level_changed {
        let reason = format!(
            "Promoted to {} after completing {} successful transaction(s)",
            new_level_str, new_successful_txs
        );

        let _: SellerTrustHistory = query_as::<_, SellerTrustHistory>(
            r#"
            INSERT INTO seller_trust_history (seller_id, old_level, new_level, reason, trigger_transaction_id)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, seller_id, old_level, new_level, reason, trigger_transaction_id, created_at
            "#
        )
        .bind(updated_profile.id)
        .bind(&old_level)
        .bind(new_level_str)
        .bind(&reason)
        .bind(order_id)
        .fetch_one(db)
        .await?;

        info!(
            "Seller {} trust level updated from {} to {} (order: {})",
            updated_profile.id, old_level, new_level_str, order_id
        );
    }

    Ok(TrustEvaluationResult {
        seller_id: updated_profile.id,
        old_trust_level: old_level,
        new_trust_level: new_level_str.to_string(),
        old_grade,
        new_grade: new_grade_str.to_string(),
        successful_transactions: new_successful_txs,
        fulfillment_rate: new_fulfillment_rate,
        level_changed,
    })
}

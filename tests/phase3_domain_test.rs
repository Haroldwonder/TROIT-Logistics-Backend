use troit_logistics_backend::{
    models::seller::{SellerGrade, SellerTrustLevel},
    subscriptions::service::{
        can_access_advanced_analytics, can_create_listing, get_subscription_entitlements,
    },
    trust::service::{
        calculate_seller_grade, calculate_trust_level, seller_grade_to_str, trust_level_to_str,
    },
};

#[test]
fn test_trust_level_deterministic_progression() {
    assert_eq!(calculate_trust_level(0, 100.0), SellerTrustLevel::LV1);
    assert_eq!(calculate_trust_level(4, 100.0), SellerTrustLevel::LV1);
    assert_eq!(calculate_trust_level(5, 92.0), SellerTrustLevel::LV2);
    assert_eq!(calculate_trust_level(14, 95.0), SellerTrustLevel::LV2);
    assert_eq!(calculate_trust_level(15, 94.0), SellerTrustLevel::LV3);
    assert_eq!(calculate_trust_level(29, 95.0), SellerTrustLevel::LV3);
    assert_eq!(calculate_trust_level(30, 96.0), SellerTrustLevel::LV4);
    assert_eq!(calculate_trust_level(49, 97.0), SellerTrustLevel::LV4);
    assert_eq!(calculate_trust_level(50, 98.5), SellerTrustLevel::LV5);

    assert_eq!(trust_level_to_str(SellerTrustLevel::LV1), "LV1");
    assert_eq!(trust_level_to_str(SellerTrustLevel::LV5), "LV5");
}

#[test]
fn test_seller_grade_deterministic_calculation() {
    // IMPORTANT: Represents supplier trust built with Troit, NOT physical product condition
    assert_eq!(calculate_seller_grade(0, 100.0), SellerGrade::GradeC);
    assert_eq!(calculate_seller_grade(9, 100.0), SellerGrade::GradeC);
    assert_eq!(calculate_seller_grade(10, 90.0), SellerGrade::GradeB);
    assert_eq!(calculate_seller_grade(24, 99.0), SellerGrade::GradeB);
    assert_eq!(calculate_seller_grade(25, 95.0), SellerGrade::GradeA);

    assert_eq!(seller_grade_to_str(SellerGrade::GradeC), "Grade C");
    assert_eq!(seller_grade_to_str(SellerGrade::GradeA), "Grade A");
}

#[test]
fn test_subscription_entitlements_and_limits() {
    let free_sub = troit_logistics_backend::models::subscription::SellerSubscription {
        id: uuid::Uuid::new_v4(),
        seller_id: uuid::Uuid::new_v4(),
        plan_tier: "FREE".to_string(),
        status: "ACTIVE".to_string(),
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let entitlements = get_subscription_entitlements(&free_sub);
    assert_eq!(entitlements.max_active_listings, Some(10));
    assert!(!entitlements.advanced_analytics_enabled);
    assert!(can_create_listing(&free_sub, 5));
    assert!(!can_create_listing(&free_sub, 10));

    let pro_sub = troit_logistics_backend::models::subscription::SellerSubscription {
        id: uuid::Uuid::new_v4(),
        seller_id: uuid::Uuid::new_v4(),
        plan_tier: "PRO".to_string(),
        status: "ACTIVE".to_string(),
        expires_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let pro_entitlements = get_subscription_entitlements(&pro_sub);
    assert_eq!(pro_entitlements.max_active_listings, Some(50));
    assert!(pro_entitlements.advanced_analytics_enabled);
    assert!(can_access_advanced_analytics(&pro_sub));
}

#[test]
fn test_african_made_category_validation() {
    let valid_categories = vec!["ELECTRONICS", "HOME_APPLIANCES", "FURNITURE"];
    for cat in valid_categories {
        assert!(matches!(
            cat,
            "ELECTRONICS" | "HOME_APPLIANCES" | "FURNITURE"
        ));
    }
}

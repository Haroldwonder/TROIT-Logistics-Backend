use crate::{
    models::subscription::SellerSubscription, subscriptions::models::SubscriptionEntitlements,
};

/// Evaluates feature entitlements central to subscription domain logic
pub fn get_subscription_entitlements(sub: &SellerSubscription) -> SubscriptionEntitlements {
    let is_active = sub.status.to_uppercase() == "ACTIVE";
    let plan = sub.plan_tier.to_uppercase();

    if !is_active {
        return SubscriptionEntitlements {
            max_active_listings: Some(5),
            advanced_analytics_enabled: false,
            business_insights_enabled: false,
            priority_verification_enabled: false,
        };
    }

    match plan.as_str() {
        "PRO" => SubscriptionEntitlements {
            max_active_listings: Some(50),
            advanced_analytics_enabled: true,
            business_insights_enabled: false,
            priority_verification_enabled: true,
        },
        "ENTERPRISE" | "VIP" => SubscriptionEntitlements {
            max_active_listings: None, // Unlimited
            advanced_analytics_enabled: true,
            business_insights_enabled: true,
            priority_verification_enabled: true,
        },
        _ => SubscriptionEntitlements {
            // Default FREE Tier
            max_active_listings: Some(10),
            advanced_analytics_enabled: false,
            business_insights_enabled: false,
            priority_verification_enabled: false,
        },
    }
}

#[allow(dead_code)]
pub fn can_create_listing(sub: &SellerSubscription, current_active_count: i64) -> bool {
    let entitlements = get_subscription_entitlements(sub);
    match entitlements.max_active_listings {
        Some(max) => current_active_count < max,
        None => true, // Unlimited
    }
}

#[allow(dead_code)]
pub fn can_access_advanced_analytics(sub: &SellerSubscription) -> bool {
    get_subscription_entitlements(sub).advanced_analytics_enabled
}

#[allow(dead_code)]
pub fn can_access_business_insights(sub: &SellerSubscription) -> bool {
    get_subscription_entitlements(sub).business_insights_enabled
}

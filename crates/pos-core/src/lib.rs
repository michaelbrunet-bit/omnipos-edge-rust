pub mod auth;
pub mod catalog;
pub mod order;
pub mod tax;
pub mod theme;

pub use auth::{
    AuthError, ManagerOverrideClaims, OperatorAccount, Permission, Role, SessionClaims, TokenSigner,
};
pub use catalog::{
    CatalogItem, CategoryConfig, ModifierOption, ModifierQualifier, PrepStationConfig,
};
pub use order::{
    ItemPrepStatus, ModAdjustment, Order, OrderLineItem, OrderSource, OrderStatus,
    OrderSummaryTotals, PaymentMethod, PaymentRecord, RefundRecord, SplitCheckShare,
};
pub use tax::{calculate_line_tax, TaxMode, TaxRule};
pub use theme::{
    BorderRadiusStyle, ColorScheme, IndustryVertical, StoreProfile, StoreTheme, ThemeError,
    UiDensity,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2id_auth_and_lockout_and_deny_by_default_customer_scope() {
        let mut op = OperatorAccount::new("op-1", "Elena", Role::Owner, "1111");
        assert!(op.authenticate_pin("1111").is_ok());

        for _ in 0..5 {
            assert!(op.authenticate_pin("0000").is_err());
        }
        // 6th attempt even with right PIN must fail while locked!
        assert!(matches!(
            op.authenticate_pin("1111"),
            Err(AuthError::AccountLocked(_))
        ));

        let signer = TokenSigner::new(b"test-pos-hmac-secret-32-bytes!!!".to_vec());
        let unscoped_customer = SessionClaims {
            jti: "jti-1".to_string(),
            actor_id: "guest".to_string(),
            actor_name: "Guest".to_string(),
            role: Role::Customer,
            store_id: "store-hospitality".to_string(),
            scoped_order_id: None,
            exp_unix: chrono::Utc::now().timestamp() + 3600,
        };
        // Deny-by-default when scoped_order_id is None!
        assert!(!unscoped_customer.can_access_order("ord-table-4"));

        let scoped_customer = SessionClaims {
            scoped_order_id: Some("ord-table-4".to_string()),
            ..unscoped_customer
        };
        let tok = signer.sign_session(&scoped_customer);
        let verified = signer.verify_session(&tok).expect("valid token");
        assert!(verified.can_access_order("ord-table-4"));
        assert!(!verified.can_access_order("ord-patio-8"));
    }

    #[test]
    fn test_mixed_exempt_and_taxable_line_item_tax_math() {
        let mut order = Order::new_for_store(
            "store-salon",
            "ord-salon-1",
            "Chair 1",
            false,
            1,
            "Devon Brooks",
        );
        // $55.00 tax-exempt service
        order.items.push(OrderLineItem {
            id: "l1".to_string(),
            catalog_item_id: "svc-1".to_string(),
            name: "Skin Fade".to_string(),
            category: "haircuts_fades".to_string(),
            quantity: 1,
            unit_price_cents: 5500,
            selected_modifiers: vec![],
            seat_number: Some(1),
            notes: None,
            added_by: "Devon".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "salon_chair".to_string(),
            prep_status: ItemPrepStatus::Fired,
            tax_rate_bps: 0,
            tax_mode: TaxMode::Exempt,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });
        // $28.00 taxable take-home pomade (8.25%)
        order.items.push(OrderLineItem {
            id: "l2".to_string(),
            catalog_item_id: "ret-1".to_string(),
            name: "Matte Clay Pomade".to_string(),
            category: "salon_retail".to_string(),
            quantity: 1,
            unit_price_cents: 2800,
            selected_modifiers: vec![],
            seat_number: Some(1),
            notes: None,
            added_by: "Devon".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "counter".to_string(),
            prep_status: ItemPrepStatus::Ready,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });

        // Apply -15% (-420 cents) discount targeted specifically at "salon_retail"
        order.mod_adjustments.push(ModAdjustment {
            mod_id: "mod-salon-bundle".to_string(),
            mod_name: "Salon Bundle".to_string(),
            label: "15% Off Retail".to_string(),
            amount_cents: -420,
            receipt_note: None,
            target_categories: vec!["salon_retail".to_string()],
            taxable_surcharge_bps: 0,
        });

        // Taxable base must be strictly $28.00 - $4.20 = $23.80 (2380 cents)
        // Tax = round(2380 * 8.25%) = round(196.35) = 196 cents ($1.96)
        assert_eq!(order.taxable_amount_cents(), 2380);
        assert_eq!(order.tax_cents(), 196);
        assert_eq!(order.total_before_tip_cents(), 5500 + 2380 + 196);
    }

    #[test]
    fn test_order_totals_and_equal_and_seat_split_exact_cents() {
        let mut order = Order::new("ord-1", "Table 7", false, 2, "Sofia Chen");
        order.items.push(OrderLineItem {
            id: "line-1".to_string(),
            catalog_item_id: "item-1".to_string(),
            name: "Craft Cocktail".to_string(),
            category: "cocktails".to_string(),
            quantity: 1,
            unit_price_cents: 1500,
            selected_modifiers: vec![],
            seat_number: Some(1),
            notes: None,
            added_by: "Sofia Chen".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "bar".to_string(),
            prep_status: ItemPrepStatus::Fired,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });
        order.items.push(OrderLineItem {
            id: "line-2".to_string(),
            catalog_item_id: "item-2".to_string(),
            name: "Wagyu Burger".to_string(),
            category: "mains".to_string(),
            quantity: 1,
            unit_price_cents: 2200,
            selected_modifiers: vec![],
            seat_number: Some(2),
            notes: None,
            added_by: "Sofia Chen".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "kitchen".to_string(),
            prep_status: ItemPrepStatus::Fired,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });

        assert_eq!(order.subtotal_cents(), 3700);
        let shares = order.split_equally(3);
        let sum_shares: i64 = shares.iter().map(|s| s.total_cents).sum();
        assert_eq!(sum_shares, order.remaining_balance_cents());

        let seat_shares = order.split_by_seat();
        assert_eq!(seat_shares.len(), 2);
        let sum_seat_shares: i64 = seat_shares.iter().map(|s| s.total_cents).sum();
        assert_eq!(sum_seat_shares, order.remaining_balance_cents());
    }
}

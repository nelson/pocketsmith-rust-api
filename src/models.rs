use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub login: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub beta_user: Option<bool>,
    pub time_zone: Option<String>,
    pub week_start_day: Option<i32>,
    pub is_reviewing_transactions: Option<bool>,
    pub base_currency_code: Option<String>,
    pub always_show_base_currency: Option<bool>,
    pub using_multiple_currencies: Option<bool>,
    pub available_accounts: Option<i32>,
    pub available_budgets: Option<i32>,
    pub forecast_last_updated_at: Option<String>,
    pub forecast_last_accessed_at: Option<String>,
    pub forecast_start_date: Option<String>,
    pub forecast_end_date: Option<String>,
    pub forecast_defer_recalculate: Option<bool>,
    pub forecast_needs_recalculate: Option<bool>,
    pub last_logged_in_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TransactionAccount {
    pub id: i64,
    pub name: Option<String>,
    pub number: Option<String>,
    pub currency_code: Option<String>,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
    pub current_balance: Option<f64>,
    pub current_balance_date: Option<String>,
    pub current_balance_in_base_currency: Option<f64>,
    pub current_balance_exchange_rate: Option<f64>,
    pub safe_balance: Option<f64>,
    pub safe_balance_in_base_currency: Option<f64>,
    pub starting_balance: Option<f64>,
    pub starting_balance_date: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Category {
    pub id: i64,
    pub title: Option<String>,
    pub colour: Option<String>,
    pub children: Option<Vec<Category>>,
    pub parent_id: Option<i64>,
    pub is_transfer: Option<bool>,
    pub is_bill: Option<bool>,
    pub roll_up: Option<bool>,
    pub refund_behaviour: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Transaction {
    pub id: i64,
    #[serde(rename = "type")]
    pub transaction_type: Option<String>,
    pub payee: Option<String>,
    pub amount: Option<f64>,
    pub amount_in_base_currency: Option<f64>,
    pub date: Option<String>,
    pub cheque_number: Option<String>,
    pub memo: Option<String>,
    pub is_transfer: Option<bool>,
    pub category: Option<Category>,
    pub note: Option<String>,
    pub labels: Option<Vec<String>>,
    pub original_payee: Option<String>,
    pub upload_source: Option<String>,
    pub closing_balance: Option<f64>,
    pub transaction_account: Option<TransactionAccount>,
    pub status: Option<String>,
    pub needs_review: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Default)]
pub struct TransactionParams {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub updated_since: Option<String>,
    pub uncategorised: Option<bool>,
    pub transaction_type: Option<String>,
    pub needs_review: Option<bool>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct TransactionUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cheque_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_transfer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_review: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct TransactionCreate {
    pub payee: String,
    pub amount: f64,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_transfer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_user_full() {
        let json = r#"{
            "id": 42,
            "login": "testuser",
            "name": "Test User",
            "email": "test@example.com",
            "avatar_url": "https://example.com/avatar.png",
            "beta_user": true,
            "time_zone": "Pacific/Auckland",
            "week_start_day": 1,
            "is_reviewing_transactions": false,
            "base_currency_code": "NZD",
            "always_show_base_currency": false,
            "using_multiple_currencies": true,
            "available_accounts": 10,
            "available_budgets": 5,
            "forecast_last_updated_at": "2024-01-01T00:00:00Z",
            "forecast_last_accessed_at": "2024-01-02T00:00:00Z",
            "forecast_start_date": "2024-01-01",
            "forecast_end_date": "2025-01-01",
            "forecast_defer_recalculate": false,
            "forecast_needs_recalculate": true,
            "last_logged_in_at": "2024-06-01T12:00:00Z",
            "last_activity_at": "2024-06-01T13:00:00Z",
            "created_at": "2020-01-01T00:00:00Z",
            "updated_at": "2024-06-01T00:00:00Z"
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 42);
        assert_eq!(user.login.as_deref(), Some("testuser"));
        assert_eq!(user.name.as_deref(), Some("Test User"));
        assert_eq!(user.email.as_deref(), Some("test@example.com"));
        assert_eq!(user.beta_user, Some(true));
        assert_eq!(user.base_currency_code.as_deref(), Some("NZD"));
        assert_eq!(user.using_multiple_currencies, Some(true));
        assert_eq!(user.available_accounts, Some(10));
    }

    #[test]
    fn test_deserialize_user_minimal() {
        let json = r#"{"id": 1}"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 1);
        assert!(user.login.is_none());
        assert!(user.name.is_none());
        assert!(user.email.is_none());
    }

    #[test]
    fn test_deserialize_user_with_nulls() {
        let json = r#"{
            "id": 1,
            "login": null,
            "name": null,
            "forecast_start_date": null,
            "forecast_end_date": null
        }"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 1);
        assert!(user.login.is_none());
        assert!(user.forecast_start_date.is_none());
    }

    #[test]
    fn test_deserialize_transaction_account() {
        let json = r#"{
            "id": 300,
            "name": "Daily Account",
            "number": "12-3456-7890123-00",
            "currency_code": "NZD",
            "type": "bank",
            "current_balance": 2500.00,
            "created_at": "2020-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;

        let ta: TransactionAccount = serde_json::from_str(json).unwrap();
        assert_eq!(ta.id, 300);
        assert_eq!(ta.number.as_deref(), Some("12-3456-7890123-00"));
        assert_eq!(ta.account_type.as_deref(), Some("bank"));
    }

    #[test]
    fn test_deserialize_category_with_children() {
        let json = r##"{
            "id": 10,
            "title": "Food",
            "colour": "#ff0000",
            "is_transfer": false,
            "is_bill": false,
            "roll_up": true,
            "children": [
                {
                    "id": 11,
                    "title": "Groceries",
                    "parent_id": 10,
                    "is_transfer": false,
                    "is_bill": false,
                    "roll_up": false
                },
                {
                    "id": 12,
                    "title": "Restaurants",
                    "parent_id": 10,
                    "is_transfer": false,
                    "is_bill": false,
                    "roll_up": false
                }
            ],
            "created_at": "2020-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }"##;

        let cat: Category = serde_json::from_str(json).unwrap();
        assert_eq!(cat.id, 10);
        assert_eq!(cat.title.as_deref(), Some("Food"));
        assert_eq!(cat.colour.as_deref(), Some("#ff0000"));
        let children = cat.children.unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].title.as_deref(), Some("Groceries"));
        assert_eq!(children[0].parent_id, Some(10));
    }

    #[test]
    fn test_deserialize_transaction_full() {
        let json = r#"{
            "id": 1000,
            "type": "debit",
            "payee": "Supermarket",
            "amount": -45.50,
            "amount_in_base_currency": -45.50,
            "date": "2024-06-15",
            "memo": "Weekly shopping",
            "is_transfer": false,
            "category": {
                "id": 11,
                "title": "Groceries",
                "is_transfer": false,
                "is_bill": false,
                "roll_up": false
            },
            "note": "Bought veggies",
            "labels": ["food", "weekly"],
            "original_payee": "SUPERMARKET #123",
            "status": "posted",
            "needs_review": false,
            "transaction_account": {
                "id": 300,
                "name": "Daily Account",
                "type": "bank"
            },
            "created_at": "2024-06-15T10:00:00Z",
            "updated_at": "2024-06-15T10:00:00Z"
        }"#;

        let txn: Transaction = serde_json::from_str(json).unwrap();
        assert_eq!(txn.id, 1000);
        assert_eq!(txn.transaction_type.as_deref(), Some("debit"));
        assert_eq!(txn.payee.as_deref(), Some("Supermarket"));
        assert_eq!(txn.amount, Some(-45.50));
        assert_eq!(txn.labels.as_ref().unwrap().len(), 2);
        assert_eq!(txn.labels.as_ref().unwrap()[0], "food");
        assert_eq!(txn.category.as_ref().unwrap().id, 11);
        assert_eq!(txn.transaction_account.as_ref().unwrap().id, 300);
        assert_eq!(txn.status.as_deref(), Some("posted"));
        assert_eq!(txn.needs_review, Some(false));
    }

    #[test]
    fn test_deserialize_transaction_minimal() {
        let json = r#"{"id": 999}"#;
        let txn: Transaction = serde_json::from_str(json).unwrap();
        assert_eq!(txn.id, 999);
        assert!(txn.transaction_type.is_none());
        assert!(txn.category.is_none());
        assert!(txn.labels.is_none());
    }

    #[test]
    fn test_serialize_transaction_update_skips_none() {
        let update = TransactionUpdate {
            payee: Some("New Payee".to_string()),
            amount: Some(-10.0),
            ..Default::default()
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"payee\":\"New Payee\""));
        assert!(json.contains("\"amount\":-10.0"));
        assert!(!json.contains("memo"));
        assert!(!json.contains("note"));
        assert!(!json.contains("labels"));
    }

    #[test]
    fn test_serialize_transaction_update_empty() {
        let update = TransactionUpdate::default();
        let json = serde_json::to_string(&update).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_transaction_params_default() {
        let params = TransactionParams::default();
        assert!(params.start_date.is_none());
        assert!(params.end_date.is_none());
        assert!(params.search.is_none());
    }

    #[test]
    fn test_deserialize_transaction_array() {
        let json = r#"[
            {"id": 1, "payee": "Store A", "amount": -10.0},
            {"id": 2, "payee": "Store B", "amount": -20.0},
            {"id": 3, "payee": "Employer", "amount": 3000.0, "type": "credit"}
        ]"#;

        let txns: Vec<Transaction> = serde_json::from_str(json).unwrap();
        assert_eq!(txns.len(), 3);
        assert_eq!(txns[0].id, 1);
        assert_eq!(txns[2].transaction_type.as_deref(), Some("credit"));
        assert_eq!(txns[2].amount, Some(3000.0));
    }

    #[test]
    fn test_deserialize_categories_array() {
        let json = r#"[
            {"id": 1, "title": "Income", "is_transfer": false, "is_bill": false, "roll_up": false},
            {"id": 2, "title": "Expenses", "is_transfer": false, "is_bill": false, "roll_up": true,
             "children": [{"id": 3, "title": "Food", "parent_id": 2, "is_transfer": false, "is_bill": false, "roll_up": false}]}
        ]"#;

        let cats: Vec<Category> = serde_json::from_str(json).unwrap();
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[1].children.as_ref().unwrap().len(), 1);
        assert_eq!(cats[1].children.as_ref().unwrap()[0].parent_id, Some(2));
    }

    #[test]
    fn test_deserialize_unknown_fields_ignored() {
        let json = r#"{
            "id": 1,
            "login": "test",
            "some_future_field": "value",
            "another_unknown": 42
        }"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 1);
        assert_eq!(user.login.as_deref(), Some("test"));
    }
}

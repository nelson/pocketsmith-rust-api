use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

pub fn upsert_category(conn: &Connection, c: &Category) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO categories (
            id, title, colour, parent_id, is_transfer, is_bill,
            roll_up, refund_behaviour, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            c.id,
            c.title,
            c.colour,
            c.parent_id,
            c.is_transfer,
            c.is_bill,
            c.roll_up,
            c.refund_behaviour,
            c.created_at,
            c.updated_at,
        ],
    )?;

    // Recursively upsert children
    if let Some(ref children) = c.children {
        for child in children {
            upsert_category(conn, child)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

    #[test]
    fn test_upsert_category_simple() {
        let conn = test_db();
        let cat = make_category(1, "Food");
        upsert_category(&conn, &cat).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM categories WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Food");
    }

    #[test]
    fn test_upsert_category_with_children() {
        let conn = test_db();
        let cat = Category {
            id: 1,
            title: Some("Food".into()),
            colour: Some("#ff0000".into()),
            children: Some(vec![
                Category {
                    id: 2,
                    title: Some("Groceries".into()),
                    colour: None,
                    children: None,
                    parent_id: Some(1),
                    is_transfer: Some(false),
                    is_bill: Some(false),
                    roll_up: Some(false),
                    refund_behaviour: None,
                    created_at: Some("2020-01-01T00:00:00Z".into()),
                    updated_at: Some("2024-01-01T00:00:00Z".into()),
                },
                Category {
                    id: 3,
                    title: Some("Restaurants".into()),
                    colour: None,
                    children: None,
                    parent_id: Some(1),
                    is_transfer: Some(false),
                    is_bill: Some(false),
                    roll_up: Some(false),
                    refund_behaviour: None,
                    created_at: Some("2020-01-01T00:00:00Z".into()),
                    updated_at: Some("2024-01-01T00:00:00Z".into()),
                },
            ]),
            parent_id: None,
            is_transfer: Some(false),
            is_bill: Some(false),
            roll_up: Some(true),
            refund_behaviour: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        };
        upsert_category(&conn, &cat).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);

        let parent_id: i64 = conn
            .query_row(
                "SELECT parent_id FROM categories WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent_id, 1);
    }

    #[test]
    fn test_upsert_category_replace() {
        let conn = test_db();
        upsert_category(&conn, &make_category(1, "Food")).unwrap();
        upsert_category(&conn, &make_category(1, "Groceries")).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM categories WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Groceries");
    }
}

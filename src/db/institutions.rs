use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

pub fn upsert_institution(conn: &Connection, i: &Institution) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO institutions (id, title, currency_code, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![i.id, i.title, i.currency_code, i.created_at, i.updated_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

    #[test]
    fn test_upsert_institution() {
        let conn = test_db();
        let inst = make_institution(1, "ANZ Bank");
        upsert_institution(&conn, &inst).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM institutions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "ANZ Bank");
    }

    #[test]
    fn test_upsert_institution_replace() {
        let conn = test_db();
        upsert_institution(&conn, &make_institution(1, "ANZ")).unwrap();
        upsert_institution(&conn, &make_institution(1, "Westpac")).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM institutions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Westpac");
    }
}

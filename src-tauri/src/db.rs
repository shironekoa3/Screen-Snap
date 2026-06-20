use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| format!("打开数据库失败: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ocr_results (
                filepath TEXT UNIQUE NOT NULL,
                ocr_text TEXT NOT NULL DEFAULT '',
                captured_at TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_ocr_filepath ON ocr_results(filepath);",
        )
        .map_err(|e| format!("初始化数据库表失败: {}", e))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_or_update_ocr(
        &self,
        filepath: &str,
        ocr_text: &str,
        captured_at: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO ocr_results (filepath, ocr_text, captured_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![filepath, ocr_text, captured_at],
        )
        .map_err(|e| format!("保存OCR结果失败: {}", e))?;
        Ok(())
    }

    pub fn search(&self, keyword: &str) -> Result<Vec<(String, String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        // Strip whitespace from keyword since OCR text has all whitespace removed
        let cleaned: String = keyword
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let like_pattern = format!("%{}%", cleaned);
        let mut stmt = conn
            .prepare(
                "SELECT filepath, ocr_text, captured_at FROM ocr_results
                 WHERE ocr_text LIKE ?1
                 ORDER BY captured_at DESC
                 LIMIT 200",
            )
            .map_err(|e| format!("准备查询失败: {}", e))?;

        let rows = stmt
            .query_map(rusqlite::params![like_pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("执行查询失败: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            if let Ok((fp, text, ts)) = row {
                // Skip orphaned entries where the file no longer exists
                if Path::new(&fp).exists() {
                    results.push((fp, text, ts));
                }
            }
        }
        Ok(results)
    }

    pub fn get_ocr_for_file(&self, filepath: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT ocr_text FROM ocr_results WHERE filepath = ?1")
            .map_err(|e| format!("准备查询失败: {}", e))?;

        let mut rows = stmt
            .query_map(rusqlite::params![filepath], |row| row.get::<_, String>(0))
            .map_err(|e| format!("执行查询失败: {}", e))?;

        match rows.next() {
            Some(Ok(text)) => Ok(Some(text)),
            Some(Err(e)) => Err(format!("读取OCR结果失败: {}", e)),
            None => Ok(None),
        }
    }

    pub fn delete_ocr_for_file(&self, filepath: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        conn.execute("DELETE FROM ocr_results WHERE filepath = ?1", rusqlite::params![filepath])
            .map_err(|e| format!("删除OCR记录失败: {}", e))?;
        Ok(())
    }
}

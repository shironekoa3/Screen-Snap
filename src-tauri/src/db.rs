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
                ocr_display TEXT NOT NULL DEFAULT '',
                captured_at TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_ocr_filepath ON ocr_results(filepath);",
        )
        .map_err(|e| format!("初始化数据库表失败: {}", e))?;

        // 老库升级：补上 ocr_display（已存在时报错，忽略即可）
        let _ = conn.execute(
            "ALTER TABLE ocr_results ADD COLUMN ocr_display TEXT NOT NULL DEFAULT ''",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// `flat` 去掉所有空白，用于搜索；`display` 保留换行，用于阅读和复制。
    pub fn insert_or_update_ocr(
        &self,
        filepath: &str,
        flat: &str,
        display: &str,
        captured_at: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO ocr_results (filepath, ocr_text, ocr_display, captured_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![filepath, flat, display, captured_at],
        )
        .map_err(|e| format!("保存OCR结果失败: {}", e))?;
        Ok(())
    }

    /// 只更新识别文本，保留原来的 captured_at（重新识别不该改变截图的排序）。
    /// 没有对应记录时按当前时间插入。
    pub fn update_ocr_text(
        &self,
        filepath: &str,
        flat: &str,
        display: &str,
        captured_at: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        let changed = conn
            .execute(
                "UPDATE ocr_results SET ocr_text = ?2, ocr_display = ?3 WHERE filepath = ?1",
                rusqlite::params![filepath, flat, display],
            )
            .map_err(|e| format!("更新OCR结果失败: {}", e))?;

        if changed == 0 {
            conn.execute(
                "INSERT INTO ocr_results (filepath, ocr_text, ocr_display, captured_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![filepath, flat, display, captured_at],
            )
            .map_err(|e| format!("保存OCR结果失败: {}", e))?;
        }
        Ok(())
    }

    pub fn search(&self, keyword: &str) -> Result<Vec<(String, String, String)>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        // 关键词也去掉空白，与 ocr_text 的存储方式保持一致
        let cleaned: String = keyword.chars().filter(|c| !c.is_whitespace()).collect();
        let like_pattern = format!("%{}%", cleaned);
        let mut stmt = conn
            .prepare(
                "SELECT filepath, ocr_text, ocr_display, captured_at FROM ocr_results
                 WHERE ocr_text LIKE ?1
                 ORDER BY captured_at DESC
                 LIMIT 200",
            )
            .map_err(|e| format!("准备查询失败: {}", e))?;

        let rows = stmt
            .query_map(rusqlite::params![like_pattern], |row| {
                let flat: String = row.get(1)?;
                let display: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    if display.is_empty() { flat } else { display },
                    row.get::<_, String>(3)?,
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

    /// 返回可读文本（保留换行）。老数据没有 ocr_display 时退回 flat。
    pub fn get_ocr_for_file(&self, filepath: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT ocr_text, ocr_display FROM ocr_results WHERE filepath = ?1")
            .map_err(|e| format!("准备查询失败: {}", e))?;

        let mut rows = stmt
            .query_map(rusqlite::params![filepath], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("执行查询失败: {}", e))?;

        match rows.next() {
            Some(Ok((flat, display))) => Ok(Some(if display.is_empty() { flat } else { display })),
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

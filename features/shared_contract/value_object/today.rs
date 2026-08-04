//! 今日日期（业务时区）。

use chrono::NaiveDate;

/// 今天的业务日期（本地时区）。
///
/// 业务单据的「日期缺省为当天」统一走本函数；用 `Local` 而非 `Utc`，
/// 避免 UTC+8 凌晨（0:00–8:00）取到前一天。
pub fn today_naive() -> NaiveDate {
    chrono::Local::now().date_naive()
}

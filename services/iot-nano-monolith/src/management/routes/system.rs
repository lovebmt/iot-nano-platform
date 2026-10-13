use super::super::*;
use sqlx::Row;

pub(in crate::management) async fn system_operational_metrics(
    store: &PlatformStore,
) -> Result<Vec<crate::SystemInfrastructureStatusRow>, ManagementSessionError> {
    let (database_size, records_label, records) = match store {
        PlatformStore::Sqlite(sqlite) => {
            let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
                .fetch_one(sqlite.pool())
                .await
                .map_err(|_| ManagementSessionError::Unavailable)?;
            let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
                .fetch_one(sqlite.pool())
                .await
                .map_err(|_| ManagementSessionError::Unavailable)?;
            let records: i64 = sqlx::query_scalar(
                "SELECT
                    (SELECT COUNT(*) FROM tenants) +
                    (SELECT COUNT(*) FROM users) +
                    (SELECT COUNT(*) FROM assets) +
                    (SELECT COUNT(*) FROM devices) +
                    (SELECT COUNT(*) FROM telemetry) +
                    (SELECT COUNT(*) FROM alert_rules) +
                    (SELECT COUNT(*) FROM business_records)",
            )
            .fetch_one(sqlite.pool())
            .await
            .map_err(|_| ManagementSessionError::Unavailable)?;
            (
                u64::try_from(page_count)
                    .unwrap_or_default()
                    .saturating_mul(u64::try_from(page_size).unwrap_or_default()),
                "Platform records".to_owned(),
                records,
            )
        }
        PlatformStore::Timescale(pool) => {
            let database_size: i64 =
                sqlx::query_scalar("SELECT pg_database_size(current_database())")
                    .fetch_one(pool)
                    .await
                    .map_err(|_| ManagementSessionError::Unavailable)?;
            let records: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(reltuples)::bigint, 0)
                 FROM pg_class
                 WHERE relname IN ('tenants', 'users', 'assets', 'devices', 'telemetry', 'alert_rules', 'business_records')",
            )
            .fetch_one(pool)
            .await
            .map_err(|_| ManagementSessionError::Unavailable)?;
            (
                u64::try_from(database_size).unwrap_or_default(),
                "Platform records (estimated)".to_owned(),
                records,
            )
        }
    };
    let disk_path = store
        .sqlite_path()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .ok_or(ManagementSessionError::Unavailable)?;
    let disk = rustix::fs::statvfs(&disk_path).map_err(|_| ManagementSessionError::Unavailable)?;
    let block_size = disk.f_frsize.max(disk.f_bsize);
    let disk_total = block_size.saturating_mul(disk.f_blocks);
    let disk_available = block_size.saturating_mul(disk.f_bavail);
    let disk_used_percent = if disk_total == 0 {
        0
    } else {
        (disk_total.saturating_sub(disk_available)).saturating_mul(100) / disk_total
    };
    Ok(vec![
        crate::SystemInfrastructureStatusRow::new("Database size", format_bytes(database_size)),
        crate::SystemInfrastructureStatusRow::new(records_label, records.to_string()),
        crate::SystemInfrastructureStatusRow::new(
            "Disk available",
            format!(
                "{} free of {} ({}% used)",
                format_bytes(disk_available),
                format_bytes(disk_total),
                disk_used_percent
            ),
        ),
    ])
}

pub(in crate::management) async fn system_tenant_metrics(
    store: &PlatformStore,
) -> Result<Vec<crate::SystemTenantMetricRow>, ManagementSessionError> {
    const TENANT_METRICS_SQL: &str = "SELECT t.slug, t.status,
        COALESCE(u.count, 0) AS users,
        COALESCE(a.count, 0) AS assets,
        COALESCE(d.count, 0) AS devices,
        COALESCE(tm.count, 0) AS telemetry,
        COALESCE(ar.count, 0) AS alerts,
        COALESCE(br.count, 0) AS business_records
        FROM tenants t
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM users GROUP BY tenant_id) u ON u.tenant_id = t.id
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM assets GROUP BY tenant_id) a ON a.tenant_id = t.id
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM devices GROUP BY tenant_id) d ON d.tenant_id = t.id
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM telemetry GROUP BY tenant_id) tm ON tm.tenant_id = t.id
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM alert_rules GROUP BY tenant_id) ar ON ar.tenant_id = t.id
        LEFT JOIN (SELECT tenant_id, COUNT(*) AS count FROM business_records GROUP BY tenant_id) br ON br.tenant_id = t.id
        ORDER BY t.slug";
    macro_rules! map_tenant_rows {
        ($rows:expr) => {
            $rows
                .into_iter()
                .map(|row| {
                    let count = |column| {
                        row.try_get::<i64, _>(column)
                            .ok()
                            .and_then(|value| u64::try_from(value).ok())
                            .unwrap_or_default()
                    };
                    Ok(crate::SystemTenantMetricRow::new(
                        row.try_get::<String, _>("slug")
                            .map_err(|_| ManagementSessionError::Unavailable)?,
                        row.try_get::<String, _>("status")
                            .map_err(|_| ManagementSessionError::Unavailable)?,
                        count("users"),
                        count("assets"),
                        count("devices"),
                        count("telemetry"),
                        count("alerts"),
                        count("business_records"),
                    ))
                })
                .collect()
        };
    }
    match store {
        PlatformStore::Sqlite(store) => {
            let rows = sqlx::query(TENANT_METRICS_SQL)
                .fetch_all(store.pool())
                .await
                .map_err(|_| ManagementSessionError::Unavailable)?;
            map_tenant_rows!(rows)
        }
        PlatformStore::Timescale(pool) => {
            let rows = sqlx::query(TENANT_METRICS_SQL)
                .fetch_all(pool)
                .await
                .map_err(|_| ManagementSessionError::Unavailable)?;
            map_tenant_rows!(rows)
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub(in crate::management) async fn platform_system(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Html<String>, ManagementSessionError> {
    let headers = request.headers().clone();
    let PlatformUiSession::System { system_account_id } =
        state.session_verifier.platform_session(&headers)?
    else {
        return Err(ManagementSessionError::Forbidden);
    };
    platform_page(
        &state,
        PlatformUiSession::System { system_account_id },
        system_lifecycle_notice(request.uri().query()),
    )
    .await
}

pub(in crate::management) async fn update_system_serial_number_length_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    let PlatformUiSession::System { system_account_id } =
        state.session_verifier.platform_session(&headers)?
    else {
        return Err(ManagementSessionError::Forbidden);
    };
    let form: SystemSerialNumberLengthForm = management_request_form(&state, request).await?;
    if !(6..=32).contains(&form.serial_number_length) {
        return Err(ManagementSessionError::BadRequest);
    }
    let updated = match state.store.as_ref() {
        PlatformStore::Sqlite(store) => {
            sqlx::query("UPDATE system_accounts SET serial_number_length = ? WHERE id = ?")
                .bind(i64::from(form.serial_number_length))
                .bind(system_account_id.to_string())
                .execute(store.pool())
                .await
                .map(|_| ())
        }
        PlatformStore::Timescale(pool) => {
            sqlx::query("UPDATE system_accounts SET serial_number_length = $1 WHERE id = $2")
                .bind(i32::from(form.serial_number_length))
                .bind(system_account_id)
                .execute(pool)
                .await
                .map(|_| ())
        }
    };
    updated.map_err(|_| ManagementSessionError::Unavailable)?;
    Ok(Redirect::to("/system"))
}

pub(in crate::management) async fn platform_system_infrastructure(
    State(state): State<ManagementState>,
    headers: HeaderMap,
) -> Result<Html<String>, ManagementSessionError> {
    let PlatformUiSession::System { system_account_id } =
        state.session_verifier.platform_session(&headers)?
    else {
        return Err(ManagementSessionError::Forbidden);
    };
    let page = state.infrastructure_status.page();
    let rendered = crate::PlatformUiRenderer::render_system_infrastructure(
        &crate::PlatformUiIdentity::new(format!("System Account {system_account_id}")),
        &page,
    )
    .map_err(|_| ManagementSessionError::Unavailable)?;
    Ok(Html(rendered))
}

pub(in crate::management) async fn platform_system_infrastructure_status(
    State(state): State<ManagementState>,
    headers: HeaderMap,
) -> Result<Html<String>, ManagementSessionError> {
    let PlatformUiSession::System { .. } = state.session_verifier.platform_session(&headers)?
    else {
        return Err(ManagementSessionError::Forbidden);
    };
    let page = state.infrastructure_status.page();
    let rendered = crate::PlatformUiRenderer::render_system_infrastructure_status(&page)
        .map_err(|_| ManagementSessionError::Unavailable)?;
    Ok(Html(rendered))
}

pub(in crate::management) fn system_tenant_status_label(status: TenantStatus) -> &'static str {
    match status {
        TenantStatus::Active => "active",
        TenantStatus::Suspended => "suspended",
        TenantStatus::Deleted => "deleted",
    }
}

#[derive(Clone, Copy)]
pub(in crate::management) enum SystemLifecycleNotice {
    TenantCreated,
    TenantSuspended,
    TenantReactivated,
    TenantDeleted,
    TenantAccountReset,
    TenantAccountDisabled,
    InvalidTenantSlug,
    InvalidTenantAccountUsername,
    InvalidTenantAccountPassword,
    InvalidRequest,
    LifecycleUnavailable,
    ServiceUnavailable,
}

impl SystemLifecycleNotice {
    const fn redirect_path(self) -> &'static str {
        match self {
            Self::TenantCreated => "/system?notice=tenant-created",
            Self::TenantSuspended => "/system?notice=tenant-suspended",
            Self::TenantReactivated => "/system?notice=tenant-reactivated",
            Self::TenantDeleted => "/system?notice=tenant-deleted",
            Self::TenantAccountReset => "/system?notice=tenant-account-reset",
            Self::TenantAccountDisabled => "/system?notice=tenant-account-disabled",
            Self::InvalidTenantSlug => "/system?notice=invalid-tenant-slug",
            Self::InvalidTenantAccountUsername => "/system?notice=invalid-tenant-account-username",
            Self::InvalidTenantAccountPassword => "/system?notice=invalid-tenant-account-password",
            Self::InvalidRequest => "/system?notice=invalid-request",
            Self::LifecycleUnavailable => "/system?notice=lifecycle-unavailable",
            Self::ServiceUnavailable => "/system?notice=service-unavailable",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::TenantCreated => "Tenant created.",
            Self::TenantSuspended => "Tenant suspended.",
            Self::TenantReactivated => "Tenant reactivated.",
            Self::TenantDeleted => "Tenant deleted.",
            Self::TenantAccountReset => "Tenant Account password reset.",
            Self::TenantAccountDisabled => "Tenant Account disabled.",
            Self::InvalidTenantSlug => {
                "Tenant slug must use lowercase letters, digits, or hyphens."
            }
            Self::InvalidTenantAccountUsername => {
                "Tenant Account username must be 3 to 64 characters using letters, digits, hyphens, or underscores."
            }
            Self::InvalidTenantAccountPassword => {
                "Tenant Account password must be at least 8 ASCII characters and include uppercase, lowercase, a number, and a symbol."
            }
            Self::InvalidRequest => "Request could not be processed.",
            Self::LifecycleUnavailable => "Tenant lifecycle change was not allowed.",
            Self::ServiceUnavailable => "Tenant lifecycle service is unavailable.",
        }
    }
}

pub(in crate::management) fn system_lifecycle_notice(query: Option<&str>) -> Option<&'static str> {
    let notice = match query {
        Some("notice=tenant-created") => SystemLifecycleNotice::TenantCreated,
        Some("notice=tenant-suspended") => SystemLifecycleNotice::TenantSuspended,
        Some("notice=tenant-reactivated") => SystemLifecycleNotice::TenantReactivated,
        Some("notice=tenant-deleted") => SystemLifecycleNotice::TenantDeleted,
        Some("notice=tenant-account-reset") => SystemLifecycleNotice::TenantAccountReset,
        Some("notice=tenant-account-disabled") => SystemLifecycleNotice::TenantAccountDisabled,
        Some("notice=invalid-tenant-slug") => SystemLifecycleNotice::InvalidTenantSlug,
        Some("notice=invalid-tenant-account-username") => {
            SystemLifecycleNotice::InvalidTenantAccountUsername
        }
        Some("notice=invalid-tenant-account-password") => {
            SystemLifecycleNotice::InvalidTenantAccountPassword
        }
        Some("notice=invalid-request") => SystemLifecycleNotice::InvalidRequest,
        Some("notice=lifecycle-unavailable") => SystemLifecycleNotice::LifecycleUnavailable,
        Some("notice=service-unavailable") => SystemLifecycleNotice::ServiceUnavailable,
        _ => return None,
    };
    Some(notice.message())
}

pub(in crate::management) fn system_lifecycle_redirect(notice: SystemLifecycleNotice) -> Redirect {
    Redirect::to(notice.redirect_path())
}

pub(in crate::management) fn system_lifecycle_form_error(
    error: ManagementSessionError,
) -> Result<Redirect, ManagementSessionError> {
    match error {
        ManagementSessionError::Unauthorized
        | ManagementSessionError::TooManyRequests
        | ManagementSessionError::Forbidden => Err(error),
        ManagementSessionError::BadRequest
        | ManagementSessionError::UnsupportedMediaType
        | ManagementSessionError::PayloadTooLarge => Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::InvalidRequest,
        )),
        ManagementSessionError::NotFound | ManagementSessionError::Conflict => Ok(
            system_lifecycle_redirect(SystemLifecycleNotice::LifecycleUnavailable),
        ),
        ManagementSessionError::Unavailable => Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::ServiceUnavailable,
        )),
    }
}

pub(in crate::management) async fn platform_stylesheet()
-> ([(axum::http::HeaderName, HeaderValue); 1], &'static str) {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        crate::platform_ui::stylesheet(),
    )
}

pub(in crate::management) async fn platform_htmx()
-> ([(axum::http::HeaderName, HeaderValue); 1], &'static str) {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        crate::platform_ui::htmx(),
    )
}

pub(in crate::management) async fn create_system_tenant(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<(StatusCode, Json<SystemTenantResponse>), ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: CreateSystemTenantRequest = management_request_json(&state, request).await?;
    let tenant_account_username = if request.tenant_account_username.is_empty() {
        request.slug.clone()
    } else {
        request.tenant_account_username.clone()
    };
    if !request.metadata.is_object() || !is_platform_username(&tenant_account_username) {
        return Err(ManagementSessionError::BadRequest);
    }
    validate_password(&request.tenant_account_password)
        .map_err(|_| ManagementSessionError::BadRequest)?;
    let password_hash = hash_password(&request.tenant_account_password)
        .map_err(|_| ManagementSessionError::Unavailable)?;
    let (tenant, tenant_account) = TenantIdentityRepository::create_tenant_with_named_account(
        state.store.as_ref(),
        NewTenant {
            slug: request.slug,
            metadata: request.metadata,
        },
        tenant_account_username,
        NewTenantAccount { password_hash },
    )
    .await
    .map_err(system_tenant_error)?;
    Ok((
        StatusCode::CREATED,
        Json(SystemTenantResponse {
            id: tenant.id,
            slug: tenant.slug,
            tenant_account_id: tenant_account.id,
        }),
    ))
}

pub(in crate::management) async fn suspend_system_tenant(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(tenant_slug): Path<String>,
) -> Result<StatusCode, ManagementSessionError> {
    require_system_account(&state.session_verifier, &headers)?;
    let tenant_id = TenantIdentityRepository::suspend_tenant(state.store.as_ref(), &tenant_slug)
        .await
        .map_err(system_tenant_error)?;
    state.session_verifier.invalidate_tenant(tenant_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::management) async fn reactivate_system_tenant(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(tenant_slug): Path<String>,
) -> Result<StatusCode, ManagementSessionError> {
    require_system_account(&state.session_verifier, &headers)?;
    TenantIdentityRepository::reactivate_tenant(state.store.as_ref(), &tenant_slug)
        .await
        .map_err(system_tenant_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::management) async fn delete_system_tenant(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(tenant_slug): Path<String>,
) -> Result<StatusCode, ManagementSessionError> {
    require_system_account(&state.session_verifier, &headers)?;
    let tenant_id = TenantIdentityRepository::delete_tenant(state.store.as_ref(), &tenant_slug)
        .await
        .map_err(system_tenant_error)?;
    state.session_verifier.invalidate_tenant(tenant_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::management) async fn reset_system_tenant_account(
    State(state): State<ManagementState>,
    Path(tenant_slug): Path<String>,
    request: Request,
) -> Result<StatusCode, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: ResetTenantAccountRequest = management_request_json(&state, request).await?;
    validate_password(&request.password).map_err(|_| ManagementSessionError::BadRequest)?;
    let password_hash =
        hash_password(&request.password).map_err(|_| ManagementSessionError::Unavailable)?;
    let tenant_account = TenantIdentityRepository::reset_tenant_account_password(
        state.store.as_ref(),
        &tenant_slug,
        password_hash,
    )
    .await
    .map_err(system_tenant_error)?;
    state
        .session_verifier
        .invalidate_tenant(tenant_account.tenant_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::management) async fn disable_system_tenant_account(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(tenant_slug): Path<String>,
) -> Result<StatusCode, ManagementSessionError> {
    require_system_account(&state.session_verifier, &headers)?;
    let tenant_account =
        TenantIdentityRepository::disable_tenant_account(state.store.as_ref(), &tenant_slug)
            .await
            .map_err(system_tenant_error)?;
    state
        .session_verifier
        .invalidate_tenant(tenant_account.tenant_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(in crate::management) async fn create_system_tenant_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: CreateSystemTenantForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    let tenant_account_username = if request.tenant_account_username.is_empty() {
        request.slug.clone()
    } else {
        request.tenant_account_username.clone()
    };
    if !is_tenant_slug(&request.slug) {
        return Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::InvalidTenantSlug,
        ));
    }
    if !is_platform_username(&tenant_account_username) {
        return Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::InvalidTenantAccountUsername,
        ));
    }
    if validate_password(&request.tenant_account_password).is_err() {
        return Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::InvalidTenantAccountPassword,
        ));
    }
    let password_hash = match hash_password(&request.tenant_account_password) {
        Ok(password_hash) => password_hash,
        Err(_) => return system_lifecycle_form_error(ManagementSessionError::Unavailable),
    };
    match TenantIdentityRepository::create_tenant_with_named_account(
        state.store.as_ref(),
        NewTenant {
            slug: request.slug,
            metadata: json!({}),
        },
        tenant_account_username,
        NewTenantAccount { password_hash },
    )
    .await
    {
        Ok(_) => Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::TenantCreated,
        )),
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

pub(in crate::management) async fn suspend_system_tenant_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: SystemTenantLifecycleForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    match TenantIdentityRepository::suspend_tenant(state.store.as_ref(), &request.slug).await {
        Ok(tenant_id) => {
            state.session_verifier.invalidate_tenant(tenant_id);
            Ok(system_lifecycle_redirect(
                SystemLifecycleNotice::TenantSuspended,
            ))
        }
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

pub(in crate::management) async fn reactivate_system_tenant_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: SystemTenantLifecycleForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    match TenantIdentityRepository::reactivate_tenant(state.store.as_ref(), &request.slug).await {
        Ok(_) => Ok(system_lifecycle_redirect(
            SystemLifecycleNotice::TenantReactivated,
        )),
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

pub(in crate::management) async fn delete_system_tenant_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: SystemTenantLifecycleForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    match TenantIdentityRepository::delete_tenant(state.store.as_ref(), &request.slug).await {
        Ok(tenant_id) => {
            state.session_verifier.invalidate_tenant(tenant_id);
            Ok(system_lifecycle_redirect(
                SystemLifecycleNotice::TenantDeleted,
            ))
        }
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

pub(in crate::management) async fn reset_system_tenant_account_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: ResetTenantAccountForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    if validate_password(&request.password).is_err() {
        return system_lifecycle_form_error(ManagementSessionError::BadRequest);
    }
    let password_hash = match hash_password(&request.password) {
        Ok(password_hash) => password_hash,
        Err(_) => return system_lifecycle_form_error(ManagementSessionError::Unavailable),
    };
    match TenantIdentityRepository::reset_tenant_account_password(
        state.store.as_ref(),
        &request.slug,
        password_hash,
    )
    .await
    {
        Ok(tenant_account) => {
            state
                .session_verifier
                .invalidate_tenant(tenant_account.tenant_id);
            Ok(system_lifecycle_redirect(
                SystemLifecycleNotice::TenantAccountReset,
            ))
        }
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

pub(in crate::management) async fn disable_system_tenant_account_form(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<Redirect, ManagementSessionError> {
    let headers = request.headers().clone();
    require_system_account(&state.session_verifier, &headers)?;
    let request: SystemTenantLifecycleForm = match management_request_form(&state, request).await {
        Ok(request) => request,
        Err(error) => return system_lifecycle_form_error(error),
    };
    match TenantIdentityRepository::disable_tenant_account(state.store.as_ref(), &request.slug)
        .await
    {
        Ok(tenant_account) => {
            state
                .session_verifier
                .invalidate_tenant(tenant_account.tenant_id);
            Ok(system_lifecycle_redirect(
                SystemLifecycleNotice::TenantAccountDisabled,
            ))
        }
        Err(error) => system_lifecycle_form_error(system_tenant_error(error)),
    }
}

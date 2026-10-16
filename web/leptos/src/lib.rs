pub mod common;
mod contracts;
use common::*;
use leptos::prelude::*;
use serde_json::{Value, json};
use xcss_web::components::{AccountPage, AdminShell};
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    xcss_web::mount(
        "root",
        || view! {<AdminShell product="xszs" landing="#instances"><Application/></AdminShell>},
    );
}
fn pairing(v: &str) -> String {
    match v {
        "pending" => t("待配对", "Awaiting pairing"),
        "paired" => t("已配对", "Paired"),
        "cancelled" => t("已取消", "Cancelled"),
        "revoked" => t("已撤销", "Revoked"),
        _ => t("未知", "Unknown"),
    }
}
#[component]
fn Application() -> impl IntoView {
    let location = route();
    let revision = RwSignal::new(0);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None);
    let selected_state = RwSignal::new(String::new());
    Effect::new(move |_| {
        let current = location.get();
        if current == "account" {
            return;
        }
        let selected = current
            .split('/')
            .nth(1)
            .and_then(|v| js_sys::decode_uri_component(v).ok())
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        selected_state.set(selected);
    });
    let selected = Signal::derive(move || selected_state.get());
    let create = Callback::new(move |()| {
        navigate("instances");
        mutation(
            "/api/v1/admin/instances".into(),
            "POST",
            Some(json!({"name":t("新实例","New instance")})),
            busy,
            error,
            move |_| {
                revision.update(|v| *v += 1);
                xcss_web::components::notify(t("实例已创建", "Instance created"));
            },
        );
    });
    view! {<div class="media-business xcss-content-stack"><HeaderActions create revision busy/><PageNav selected/><Failure error message=t("实例未能创建，请刷新列表核对后重试。","The instance could not be created. Refresh the list before retrying.")/>{move||{let p=location.get();if p=="account"{view!{<AccountPage/>}.into_any()}else if p.starts_with("logs"){view!{<Logs user_id=selected.get() revision/>}.into_any()}else if p.starts_with("details/"){view!{<Details id=selected.get() revision/>}.into_any()}else{view!{<Overview revision/>}.into_any()}}}</div>}
}
#[component]
fn Overview(revision: RwSignal<u64>) -> impl IntoView {
    let cursor = RwSignal::new(String::new());
    let path = Signal::derive(move || {
        let c = cursor.get();
        format!(
            "/api/v1/admin/overview{}",
            if c.is_empty() {
                String::new()
            } else {
                format!("?cursor={}", enc(&c))
            }
        )
    });
    let (data, error) = resource(path, revision, 0);
    let candidate = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let action_error = RwSignal::new(None);
    Effect::new(move |_| {
        revision.get();
        cursor.set(String::new());
    });
    view! {<h1 class="xcss-visually-hidden">{t("备份实例列表","Backup instance list")}</h1><Failure error message=t("备份数据暂不可用，请重试。","Backup data is temporarily unavailable. Please retry.")/><Failure error=action_error message=t("删除未能确认，请刷新实例列表核对。","Deletion could not be confirmed. Refresh and check the instance list.")/>{move||data.get().map(|v|view!{<div class="media-sections"><section class="xcss-content-stack"><div class="xcss-table-scroll"><table class="xcss-table xcss-statistics-table" aria-label=t("实例统计","Instance statistics")><thead><tr><th>{t("统计项","Metric")}</th><th>{t("总数 / 在线","Total / online")}</th></tr></thead><tbody><tr><th>{t("总数","Total")}</th><td>{format!("{} / {}",num(&v,"total_users"),num(&v,"online_users"))}</td></tr><tr><th>{t("媒体已用","Media storage used")}</th><td>{bytes(num(&v,"used_bytes"))}</td></tr><tr><th>{t("上传预留空间","Reserved upload space")}</th><td>{bytes(num(&v,"pending_bytes"))}</td></tr><tr><th>{t("已分配配额","Allocated quota")}</th><td>{format!("{}{}",bytes(num(&v,"quota_bytes")),if num(&v,"unlimited_users")>0.{t(" + 不限"," + Unlimited")}else{String::new()})}</td></tr></tbody></table></div></section><section class="xcss-content-stack" aria-label=t("实例","Instances")><div class="xcss-table-scroll"><table class="xcss-table" aria-label=t("实例列表","Instance list")><thead><tr>{[t("实例","Instance"),t("配对状态","Pairing status"),t("在线状态","Online status"),t("操作系统/架构","Operating system / architecture"),t("最后在线","Last seen"),t("已用容量 / 配额","Used / quota"),t("删除","Delete")].into_iter().map(|v|view!{<th>{v}</th>}).collect_view()}</tr></thead><tbody>{rows(&v,"users").into_iter().map(|u|{let client=rows(&u,"instances").first().cloned().unwrap_or(Value::Null);let id=text(&client,"id");let i2=id.clone();let i3=id.clone();view!{<tr><th scope="row"><a class="xcss-instance-link" href=format!("#details/{}",enc(&text(&u,"id")))>{text(&u,"display_name")}</a></th><td>{pairing(&text(&client,"status"))}</td><td>{if flag(&client,"online"){t("在线","Online")}else{t("离线","Offline")}}</td><td>{text(&client,"platform")}</td><td>{timestamp(&text(&client,"last_seen_at"))}</td><td>{format!("{} / {}",bytes(num(&u,"used_bytes")),if num(&u,"quota_bytes")==0.{t("不限","Unlimited")}else{bytes(num(&u,"quota_bytes"))})}</td><td><div class="xcss-actions">{move||if candidate.get()==id{let target=i2.clone();view!{<button class="xcss-button" disabled=move||busy.get() on:click=move |_|candidate.set(String::new())>{t("取消","Cancel")}</button><button class="xcss-button xcss-danger" disabled=move||busy.get() on:click=move |_|mutation(format!("/api/v1/admin/instances/{target}"),"DELETE",None,busy,action_error,move |_|{candidate.set(String::new());cursor.set(String::new());revision.update(|v|*v+=1)})>{t("确认删除","Confirm delete")}</button>}.into_any()}else{let target=i3.clone();view!{<button class="xcss-button" disabled=move||busy.get() on:click=move |_|candidate.set(target.clone())>{t("删除","Delete")}</button>}.into_any()}}</div></td></tr>}}).collect_view()}</tbody></table></div></section></div>})}<Pagination data cursor revision/>}
}
fn quota_bytes(v: &str) -> Option<u64> {
    let n = v.trim().parse::<f64>().ok()?;
    let b = (n * 1_073_741_824.).round();
    if n.is_finite() && n >= 0. && (n == 0. || b > 0.) && b <= 9_007_199_254_740_991. {
        Some(b as u64)
    } else {
        None
    }
}
#[component]
fn Details(id: String, revision: RwSignal<u64>) -> impl IntoView {
    let p = format!("/api/v1/admin/users/{}", enc(&id));
    let (data, error) = resource(Signal::derive(move || p.clone()), revision, 0);
    view! {<h1 class="xcss-visually-hidden">{t("备份详细信息","Backup details")}</h1><Failure error message=t("备份数据暂不可用，请重试。","Backup data is temporarily unavailable. Please retry.")/><For each={move||data.get().into_iter().collect::<Vec<_>>()} key=|u|text(u,"id") children=move|initial|{let user=Signal::derive(move||data.get().unwrap_or_else(||initial.clone()));view!{<UserDetails user revision data/>}}/>}
}
#[component]
fn UserDetails(
    user: Signal<Value>,
    revision: RwSignal<u64>,
    data: RwSignal<Option<Value>>,
) -> impl IntoView {
    let initial = user.get_untracked();
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None);
    let dialog = RwSignal::new(String::new());
    let name = RwSignal::new(text(&initial, "display_name"));
    let storage = RwSignal::new(text(&initial, "storage_path"));
    let quota = RwSignal::new((num(&initial, "quota_bytes") / 1_073_741_824.).to_string());
    let id = StoredValue::new(text(&initial, "id"));
    let saved_quota = RwSignal::new((
        quota.get_untracked(),
        initial["quota_bytes"].as_u64().unwrap_or(0),
    ));
    let instance = Signal::derive(move || {
        rows(&user.get(), "instances")
            .first()
            .cloned()
            .unwrap_or(Value::Null)
    });
    let status = Signal::derive(move || text(&instance.get(), "status"));
    let terminal = Signal::derive(move || matches!(status.get().as_str(), "cancelled" | "revoked"));
    view! {
        <div class="xcss-content-stack">
            <section class="xcss-content-panel" aria-label=t("配对账户信息", "Pairing account information")>
                <h2>{move || text(&user.get(), "display_name")}</h2>
                <dl class="media-detail-list">
                    <dt>{t("实例名称", "Instance name")}</dt><dd>{move || text(&user.get(), "display_name")}</dd>
                    <dt>{t("实例 ID", "Instance ID")}</dt><dd><code>{move || text(&instance.get(), "id")}</code></dd>
                    <dt>{t("密码", "Password")}</dt><dd><code>{move || text(&instance.get(), "authorization_code")}</code></dd>
                    <dt>{t("配对状态", "Pairing status")}</dt><dd>{move || pairing(&status.get())}</dd>
                </dl>
            </section>
            <section class="xcss-content-panel xcss-content-stack" aria-label=t("备份状态", "Backup status")>
                <h2>{t("备份状态", "Backup status")}</h2>
                {move || rows(&user.get(), "instances").into_iter().map(|i| view! {
                    <dl class="media-detail-list"><dt>{t("客户端", "Client")}</dt><dd>{text(&i, "name")}</dd>
                        <dt>{t("平台", "Platform")}</dt><dd>{text(&i, "platform")}</dd>
                        <dt>{t("在线状态", "Online status")}</dt><dd>{if flag(&i, "online") { t("在线", "Online") } else { t("离线", "Offline") }}</dd>
                        <dt>{t("最后在线", "Last seen")}</dt><dd>{timestamp(&text(&i, "last_seen_at"))}</dd>
                    </dl>
                }).collect_view()}
            </section>
            <section class="media-card xcss-content-panel" aria-label=t("实例设置", "Instance settings")>
                <h2>{t("实例设置", "Instance settings")}</h2>
                <form aria-label=move || format!("{}{}", t("编辑备份实例 ", "Edit backup instance "), text(&user.get(), "display_name"))
                    aria-busy=move || busy.get().to_string()
                    on:submit=move |ev| {
                        ev.prevent_default();
                        let raw = quota.get_untracked();
                        let (saved_raw, saved_bytes) = saved_quota.get_untracked();
                        let amount = if raw.trim() == saved_raw { Some(saved_bytes) } else { quota_bytes(&raw) };
                        if let Some(amount) = amount {
                            mutation(format!("/api/v1/admin/users/{}", enc(&id.get_value())), "PUT", Some(json!({"username":text(&user.get_untracked(), "username"),"display_name":name.get_untracked(),"storage_path":storage.get_untracked(),"quota_bytes":amount})), busy, error, move |v| {
                                saved_quota.set((raw, v["quota_bytes"].as_u64().unwrap_or(amount)));
                                data.set(Some(v));
                                xcss_web::components::notify(t("实例设置已保存", "Instance settings saved"));
                            });
                        }
                    }>
                    <Failure error message=t("未能保存备份实例，请检查名称、路径和配额后重试。", "Unable to save the backup instance. Check the name, path, and quota, then retry.")/>
                    <label class="xcss-form-field">{t("实例名称", "Instance name")}<input class="xcss-input" name="display_name" prop:value=move || name.get() required readonly=move || busy.get() on:input=move |ev| name.set(event_target_value(&ev))/></label>
                    <label class="xcss-form-field">{t("存储路径", "Storage path")}<input class="xcss-input" name="storage_path" prop:value=move || storage.get() required readonly=move || busy.get() on:input=move |ev| storage.set(event_target_value(&ev))/></label>
                    <label class="xcss-form-field">{t("配额（GiB，0 表示不限）", "Quota (GiB; 0 means unlimited)")}<input class="xcss-input" name="quota_gib" type="number" min="0" step="any" prop:value=move || quota.get() required readonly=move || busy.get() on:input=move |ev| quota.set(event_target_value(&ev))/></label>
                    <div class="xcss-actions"><button class="xcss-button" type="submit" disabled=move || busy.get() || !valid_name(&name.get()) || quota_bytes(&quota.get()).is_none()>{move || if busy.get() { t("正在保存…", "Saving…") } else { t("保存设置", "Save settings") }}</button></div>
                </form>
            </section>
            <section class="xcss-content-panel xcss-content-stack" aria-label=t("实例操作", "Instance actions")>
                <h2>{t("实例操作", "Instance actions")}</h2>
                <p>{t("实例拥有一个长期客户端密码。服务端加密保存并可查看；更换后旧客户端立即失效并需要重新配对。", "The instance owns one long-lived client password. The server stores it encrypted and keeps it viewable; changing it invalidates the old client and requires pairing again.")}</p>
                <div class="xcss-actions">
                    <Show when=move || !terminal.get()><button class="xcss-button" disabled=move || busy.get() on:click=move |_| { error.set(None); dialog.set("rotate".into()); }>{t("更换密码", "Change password")}</button></Show>
                    <button class="xcss-button" disabled=move || busy.get() on:click=move |_| { error.set(None); dialog.set("remove".into()); }>{move || if status.get() == "pending" { t("取消配对", "Cancel pairing") } else if terminal.get() { t("删除实例", "Delete instance") } else { t("撤销实例", "Revoke instance") }}</button>
                </div>
            </section>
            {move || (!dialog.get().is_empty()).then(|| {
                let rotating = dialog.get() == "rotate";
                let target = text(&instance.get_untracked(), "id");
                view! {
                    <Confirm title=if rotating { t("更换密码", "Change password") } else if terminal.get_untracked() { t("删除实例", "Delete instance") } else { t("取消或撤销实例", "Cancel or revoke instance") }
                        description=if rotating { t("更换密码会立即撤销当前客户端凭据。客户端必须使用新密码重新配对。", "Changing the password immediately revokes the current client credential. The client must pair again using the new password.") } else if terminal.get_untracked() { t("永久删除这条终态且没有备份数据的实例信息。", "Permanently delete this terminal instance entry when it owns no backup data.") } else { t("密码和访问令牌将立即失效。终态实例之后可从列表永久删除。", "The password and access token are invalidated immediately. The terminal instance can then be permanently deleted from the list.") }
                        error_message=if rotating { t("更换未能确认，请刷新核对密码后再操作。", "The change could not be confirmed. Refresh and verify the password before another action.") } else { t("操作未能确认，请刷新核对实例状态。", "The operation could not be confirmed. Refresh and check the instance state.") }
                        error busy close=Callback::new(move |()| dialog.set(String::new()))
                        confirm=Callback::new(move |()| {
                            let path = format!("/api/v1/admin/instances/{target}{}", if rotating { "/authorization" } else { "" });
                            mutation(path, if rotating { "PUT" } else { "DELETE" }, None, busy, error, move |_| { dialog.set(String::new()); revision.update(|v| *v += 1); });
                        })/>
                }
            })}
        </div>
    }
}

#[component]
fn Logs(user_id: String, revision: RwSignal<u64>) -> impl IntoView {
    let selected = RwSignal::new(user_id.clone());
    let p = if user_id.is_empty() {
        String::new()
    } else {
        format!("/api/v1/admin/users/{}", enc(&user_id))
    };
    let (user, user_error) = resource(Signal::derive(move || p.clone()), revision, 0);
    let range = RwSignal::new((String::new(), String::new()));
    let range_valid = RwSignal::new(false);
    let applied = RwSignal::new((String::new(), String::new()));
    let cursor = RwSignal::new(String::new());
    let local = RwSignal::new(0);
    let instance = Signal::derive(move || {
        user.get()
            .and_then(|u| rows(&u, "instances").first().cloned())
    });
    let path = Signal::derive(move || {
        revision.get();
        let mut q = Vec::new();
        let (s, e) = applied.get();
        if !s.is_empty() {
            if s == e {
                q.push(format!("date={}", enc(&s)))
            } else {
                q.push(format!("start_date={}&end_date={}", enc(&s), enc(&e)))
            }
        }
        let c = cursor.get();
        if !c.is_empty() {
            q.push(format!("cursor={}", enc(&c)))
        }
        if !selected.get().is_empty() {
            let Some(i) = instance.get() else {
                return String::new();
            };
            q.push(format!("instance_id={}", enc(&text(&i, "id"))))
        }
        format!(
            "/api/v1/admin/logs{}",
            if q.is_empty() {
                String::new()
            } else {
                format!("?{}", q.join("&"))
            }
        )
    });
    let (data, error) = resource(path, local, 0);
    Effect::new(move |_| {
        if let Some(d) = data.get()
            && applied.get_untracked().0.is_empty()
        {
            let s = text(&d, "date");
            let e = d
                .get("end_date")
                .and_then(Value::as_str)
                .unwrap_or(&s)
                .to_string();
            range.set((s, e));
        }
    });
    view! {<section class="xcss-content-stack" aria-label=t("备份日志","Backup logs")><div class="xcss-content-panel xcss-content-stack"><p>{move||format!("{}{}",t("实例：","Instance: "),instance.get().map(|i|text(&i,"name")).unwrap_or(t("全部实例","All instances")))}</p><div class="xcss-log-date-controls"><label for="media-log-date-start-year">{t("日志日期范围（UTC）","Log date range (UTC)")}</label><div class="xcss-actions">{(!user_id.is_empty()).then(||view!{<button class="xcss-button" on:click=move |_|navigate("logs")>{t("全部实例","All instances")}</button>})}<button class="xcss-button" disabled=move||!range_valid.get() on:click=move |_|{cursor.set(String::new());local.update(|v|*v+=1)}>{t("刷新日志","Refresh logs")}</button></div><DateRange id="media-log-date" valid=range_valid value=range apply=Callback::new(move|r:(String,String)|{range.set(r.clone());applied.set(r);cursor.set(String::new());local.update(|v|*v+=1);})/></div></div><Failure error=user_error/><Failure error retry=Callback::new(move|()|{cursor.set(String::new());local.update(|v|*v+=1);}) message=t("日志暂不可用，请重试。","Logs are temporarily unavailable. Please retry.")/>{move||data.get().filter(|_|error.get().is_none()).map(|d|view!{<div class="xcss-table-scroll"><table class="xcss-table" aria-label=t("备份日志记录","Backup log records")><thead><tr><th>{t("时间（UTC）","Time (UTC)")}</th><th>{t("操作","Action")}</th><th>{t("对象","Entity")}</th></tr></thead><tbody>{rows(&d,"logs").into_iter().map(|l|view!{<tr><td><time>{text(&l,"occurred_at")}</time></td><td>{action_label(&text(&l,"action"))}" "<code>{text(&l,"action")}</code></td><td>{text(&l,"entity_id")}</td></tr>}).collect_view()}</tbody></table></div>})}<Pagination data cursor revision=local label=t("日志分页","Log pages")/></section>}
}
fn action_label(v: &str) -> String {
    match v {
        "device.instance.create" => t("创建实例", "Create instance"),
        "device.instance.delete" => t("删除实例", "Delete instance"),
        "device.instance.revoke" => t("撤销实例", "Revoke instance"),
        "device.authorization.rotate" => t("更换密码", "Change password"),
        "device.pairing.cancel" => t("取消配对", "Cancel pairing"),
        "device.bootstrap" => t("配对实例", "Pair instance"),
        "upload.commit" => t("完成上传", "Complete upload"),
        "upload.superseded" => t("上传已被替代", "Upload superseded"),
        "asset.update" => t("更新媒体", "Update media"),
        "asset.trash" => t("移入回收站", "Move to trash"),
        "asset.restore" => t("恢复媒体", "Restore media"),
        "asset.delete" => t("删除媒体", "Delete media"),
        "album.sync" => t("同步相册", "Sync album"),
        "tag.create" => t("创建标签", "Create tag"),
        "tag.assets.set" => t("设置媒体标签", "Set media tags"),
        "tag.asset.add" => t("添加媒体标签", "Add media tag"),
        "tag.asset.remove" => t("移除媒体标签", "Remove media tag"),
        "resource.deduplicate" => t("复用已有文件", "Reuse existing file"),
        "api_key.create" => t("创建 API 密钥", "Create API key"),
        "api_key.revoke" => t("撤销 API 密钥", "Revoke API key"),
        _ => t("未识别的操作", "Unrecognized action"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_quota_rounding() {
        assert_eq!(quota_bytes("0.1"), Some(107374182));
        assert_eq!(quota_bytes(""), None);
        assert_eq!(quota_bytes("-1"), None);
        assert_eq!(quota_bytes("0.00000000001"), None);
    }
    #[test]
    fn real_calendar_dates() {
        assert!(date_validate(&["2024".into(), "2".into(), "29".into()]).is_ok());
        assert_eq!(
            date_validate(&["2023".into(), "2".into(), "29".into()]),
            Err(2)
        );
    }
    #[test]
    fn labels_and_unzoned_last_seen_preserve_the_original_contract() {
        for (value, expected) in [
            ("pending", "Awaiting pairing"),
            ("paired", "Paired"),
            ("cancelled", "Cancelled"),
            ("revoked", "Revoked"),
            ("future_state", "Unknown"),
            ("__proto__", "Unknown"),
        ] {
            assert_eq!(pairing(value), expected);
        }
        for (value, expected) in [
            ("device.authorization.rotate", "Change password"),
            ("asset.trash", "Move to trash"),
            ("future.action", "Unrecognized action"),
            ("constructor", "Unrecognized action"),
        ] {
            assert_eq!(action_label(value), expected);
        }
        assert_eq!(timestamp("2026-10-08 08:00:00"), "Unknown");
        assert_eq!(timestamp("invalid-date"), "Unknown");
        assert_eq!(timestamp(""), "Not paired yet");
        assert!(has_timezone("2026-10-08T08:00:00Z"));
        assert!(has_timezone("2026-10-08T16:00:00+08:00"));
    }
}

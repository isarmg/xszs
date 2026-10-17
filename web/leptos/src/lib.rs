use leptos::component::{component_props_builder, component_view};
pub mod common;
mod contracts;
mod models;
use common::*;
use leptos::prelude::*;
use models::*;
use serde_json::{Value, json};
use xcss_web::components::{AccountPage, AdminShell};
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    xcss_web::mount("root", || {
        View::new({
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &AdminShell,
                {
                    let mut props = component_props_builder(&AdminShell)
                        .product(
                            #[allow(unused_braces)]
                            {
                                "xszs"
                            },
                        )
                        .landing(
                            #[allow(unused_braces)]
                            {
                                "#instances"
                            },
                        )
                        .children({
                            ::leptos::children::ToChildren::to_children(move || {
                                #[allow(unreachable_code)]
                                #[allow(unused_mut)]
                                #[allow(
                                    clippy::let_and_return,
                                    clippy::let_unit_value,
                                    clippy::unit_arg
                                )]
                                component_view(
                                    #[allow(clippy::needless_borrows_for_generic_args)]
                                    &Application,
                                    {
                                        let mut props =
                                            component_props_builder(&Application).build();
                                        props
                                    },
                                )
                            })
                        })
                        .build();
                    props
                },
            )
        })
    });
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
    View::new(
    leptos::html::div()
        .child((
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &HeaderActions,
                    {
                        let mut props = component_props_builder(
                                &HeaderActions,
                            )
                            .create(#[allow(unused_braces)] { create })
                            .revision(#[allow(unused_braces)] { revision })
                            .busy(#[allow(unused_braces)] { busy })
                            .build();
                        props
                    },
                )
            },
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &PageNav,
                    {
                        let mut props = component_props_builder(
                                &PageNav,
                            )
                            .selected(#[allow(unused_braces)] { selected })
                            .build();
                        props
                    },
                )
            },
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &Failure,
                    {
                        let mut props = component_props_builder(
                                &Failure,
                            )
                            .error(#[allow(unused_braces)] { error })
                            .message(
                                #[allow(unused_braces)]
                                {
                                    t(
                                        "实例未能创建，请刷新列表核对后重试。",
                                        "The instance could not be created. Refresh the list before retrying.",
                                    )
                                },
                            )
                            .build();
                        props
                    },
                )
            },
            IntoRender::into_render({
                move || {
                    let p = location.get();
                    if p == "account" {
                        View::new({
    #[allow(unreachable_code)] #[allow(unused_mut)] #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
    component_view(
        #[allow(clippy::needless_borrows_for_generic_args)]
        &AccountPage,
        {
            let mut props = component_props_builder(
                    &AccountPage,
                )
                .build();
            props
        },
    )
})
                            .into_any()
                    } else if p.starts_with("logs") {
                        View::new({
    #[allow(unreachable_code)] #[allow(unused_mut)] #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
    component_view(
        #[allow(clippy::needless_borrows_for_generic_args)]
        &Logs,
        {
            let mut props = component_props_builder(&Logs)
                .user_id(#[allow(unused_braces)] { selected.get() })
                .revision(#[allow(unused_braces)] { revision })
                .build();
            props
        },
    )
})
                            .into_any()
                    } else if p.starts_with("details/") {
                        View::new({
    #[allow(unreachable_code)] #[allow(unused_mut)] #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
    component_view(
        #[allow(clippy::needless_borrows_for_generic_args)]
        &Details,
        {
            let mut props = component_props_builder(&Details)
                .id(#[allow(unused_braces)] { selected.get() })
                .revision(#[allow(unused_braces)] { revision })
                .build();
            props
        },
    )
})
                            .into_any()
                    } else {
                        View::new({
    #[allow(unreachable_code)] #[allow(unused_mut)] #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
    component_view(
        #[allow(clippy::needless_borrows_for_generic_args)]
        &Overview,
        {
            let mut props = component_props_builder(&Overview)
                .revision(#[allow(unused_braces)] { revision })
                .build();
            props
        },
    )
})
                            .into_any()
                    }
                }
            }),
        ))
        .class("media-business xcss-content-stack"),
)
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
    let (data, error) = resource_typed::<models::Overview>(path, revision, 0);
    let candidate = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let action_error = RwSignal::new(None);
    Effect::new(move |_| {
        revision.get();
        cursor.set(String::new());
    });
    View::new((
        leptos::html::h1()
            .child(
                #[allow(unused_braces)]
                {
                    IntoRender::into_render({ t("备份实例列表", "Backup instance list") })
                },
            )
            .class("xcss-visually-hidden"),
        {
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &Failure,
                {
                    let mut props = component_props_builder(&Failure)
                        .error(
                            #[allow(unused_braces)]
                            {
                                error
                            },
                        )
                        .message(
                            #[allow(unused_braces)]
                            {
                                t(
                                    "备份数据暂不可用，请重试。",
                                    "Backup data is temporarily unavailable. Please retry.",
                                )
                            },
                        )
                        .build();
                    props
                },
            )
        },
        {
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &Failure,
                {
                    let mut props = component_props_builder(
                        &Failure,
                    )
                    .error(#[allow(unused_braces)] { action_error })
                    .message(
                        #[allow(unused_braces)]
                        {
                            t(
                                "删除未能确认，请刷新实例列表核对。",
                                "Deletion could not be confirmed. Refresh and check the instance list.",
                            )
                        },
                    )
                    .build();
                    props
                },
            )
        },
        IntoRender::into_render({
            move || {
                data
                .get()
                .map(|v| {
                    View::new(
    leptos::html::div()
        .child((
            leptos::html::section()
                .child(
                    #[allow(unused_braces)]
                    {
                        leptos::html::div()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    leptos::html::table()
                                        .child((
                                            leptos::html::thead()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        leptos::html::tr()
                                                            .child((
                                                                leptos::html::th()
                                                                    .child(
                                                                        #[allow(unused_braces)]
                                                                        {
                                                                            IntoRender::into_render({
                                                                                t("统计项", "Metric")
                                                                            })
                                                                        },
                                                                    ),
                                                                leptos::html::th()
                                                                    .child(
                                                                        #[allow(unused_braces)]
                                                                        {
                                                                            IntoRender::into_render({
                                                                                t("总数 / 在线", "Total / online")
                                                                            })
                                                                        },
                                                                    ),
                                                            ))
                                                    },
                                                ),
                                            leptos::html::tbody()
                                                .child((
                                                    leptos::html::tr()
                                                        .child((
                                                            leptos::html::th()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            t("总数", "Total")
                                                                        })
                                                                    },
                                                                ),
                                                            leptos::html::td()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            format!("{} / {}", v.total_users, v.online_users)
                                                                        })
                                                                    },
                                                                ),
                                                        )),
                                                    leptos::html::tr()
                                                        .child((
                                                            leptos::html::th()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            t("媒体已用", "Media storage used")
                                                                        })
                                                                    },
                                                                ),
                                                            leptos::html::td()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            bytes(v.used_bytes as f64)
                                                                        })
                                                                    },
                                                                ),
                                                        )),
                                                    leptos::html::tr()
                                                        .child((
                                                            leptos::html::th()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            t("上传预留空间", "Reserved upload space")
                                                                        })
                                                                    },
                                                                ),
                                                            leptos::html::td()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            bytes(v.pending_bytes as f64)
                                                                        })
                                                                    },
                                                                ),
                                                        )),
                                                    leptos::html::tr()
                                                        .child((
                                                            leptos::html::th()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            t("已分配配额", "Allocated quota")
                                                                        })
                                                                    },
                                                                ),
                                                            leptos::html::td()
                                                                .child(
                                                                    #[allow(unused_braces)]
                                                                    {
                                                                        IntoRender::into_render({
                                                                            format!(
                                                                                "{}{}", bytes(v.quota_bytes as f64), if v.unlimited_users >
                                                                                0 { t(" + 不限", " + Unlimited") } else { String::new() }
                                                                            )
                                                                        })
                                                                    },
                                                                ),
                                                        )),
                                                )),
                                        ))
                                        .class("xcss-table xcss-statistics-table")
                                        .aria_label(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                                                t("实例统计", "Instance statistics"),
                                            ),
                                        )
                                },
                            )
                            .class("xcss-table-scroll")
                    },
                )
                .class("xcss-content-stack"),
            leptos::html::section()
                .child(
                    #[allow(unused_braces)]
                    {
                        leptos::html::div()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    leptos::html::table()
                                        .child((
                                            leptos::html::thead()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        leptos::html::tr()
                                                            .child(
                                                                #[allow(unused_braces)]
                                                                {
                                                                    IntoRender::into_render({
                                                                        [
                                                                            t("实例", "Instance"),
                                                                            t("配对状态", "Pairing status"),
                                                                            t("在线状态", "Online status"),
                                                                            t("操作系统/架构", "Operating system / architecture"),
                                                                            t("最后在线", "Last seen"),
                                                                            t("已用容量 / 配额", "Used / quota"),
                                                                            t("删除", "Delete"),
                                                                        ]
                                                                            .into_iter()
                                                                            .map(|v| {
                                                                                View::new(
    leptos::html::th()
        .child(
            #[allow(unused_braces)]
            { IntoRender::into_render({ v }) },
        ),
)
                                                                            })
                                                                            .collect_view()
                                                                    })
                                                                },
                                                            )
                                                    },
                                                ),
                                            leptos::html::tbody()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        IntoRender::into_render({
                                                            v.users
                                                                .into_iter()
                                                                .map(|u| {
                                                                    let client = u.instances.first().cloned();
                                                                    let id = client
                                                                        .as_ref()
                                                                        .map(|i| i.id.clone())
                                                                        .unwrap_or("—".into());
                                                                    let i2 = id.clone();
                                                                    let i3 = id.clone();
                                                                    View::new(
    leptos::html::tr()
        .child((
            leptos::html::th()
                .child(
                    #[allow(unused_braces)]
                    {
                        leptos::html::a()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    IntoRender::into_render({
                                        u.display_name.clone()
                                    })
                                },
                            )
                            .class("xcss-instance-link")
                            .href(
                                ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                                    format!("#details/{}", enc(& u.id.clone())),
                                ),
                            )
                    },
                )
                .scope("row"),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            pairing(
                                &client
                                    .as_ref()
                                    .map(|i| i.status.clone())
                                    .unwrap_or("—".into()),
                            )
                        })
                    },
                ),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            if client.as_ref().is_some_and(|i| i.online) {
                                t("在线", "Online")
                            } else {
                                t("离线", "Offline")
                            }
                        })
                    },
                ),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            client
                                .as_ref()
                                .map(|i| i.platform.clone())
                                .unwrap_or("—".into())
                        })
                    },
                ),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            timestamp(
                                &client
                                    .as_ref()
                                    .map(|i| i.last_seen_at.clone())
                                    .unwrap_or("—".into()),
                            )
                        })
                    },
                ),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            format!(
                                "{} / {}", bytes(u.used_bytes as f64), if u.quota_bytes == 0
                                { t("不限", "Unlimited") } else { bytes(u.quota_bytes as
                                f64) }
                            )
                        })
                    },
                ),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        leptos::html::div()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    IntoRender::into_render({
                                        move || {
                                            if candidate.get() == id {
                                                let target = i2.clone();
                                                View::new((
    leptos::html::button()
        .child(
            #[allow(unused_braces)]
            { IntoRender::into_render({ t("取消", "Cancel") }) },
        )
        .class("xcss-button")
        .disabled(
            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move || {
                busy.get()
            }),
        )
        .on(
            leptos::ev::click,
            move |_| candidate.set(String::new()),
        ),
    leptos::html::button()
        .child(
            #[allow(unused_braces)]
            {
                IntoRender::into_render({
                    t("确认删除", "Confirm delete")
                })
            },
        )
        .class("xcss-button xcss-danger")
        .disabled(
            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move || {
                busy.get()
            }),
        )
        .on(
            leptos::ev::click,
            move |_| mutation(
                format!("/api/v1/admin/instances/{target}"),
                "DELETE",
                None,
                busy,
                action_error,
                move |_| {
                    candidate.set(String::new());
                    cursor.set(String::new());
                    revision.update(|v| *v += 1)
                },
            ),
        ),
))
                                                    .into_any()
                                            } else {
                                                let target = i3.clone();
                                                View::new(
    leptos::html::button()
        .child(
            #[allow(unused_braces)]
            { IntoRender::into_render({ t("删除", "Delete") }) },
        )
        .class("xcss-button")
        .disabled(
            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move || {
                busy.get()
            }),
        )
        .on(
            leptos::ev::click,
            move |_| candidate.set(target.clone()),
        ),
)
                                                    .into_any()
                                            }
                                        }
                                    })
                                },
                            )
                            .class("xcss-actions")
                    },
                ),
        )),
)
                                                                })
                                                                .collect_view()
                                                        })
                                                    },
                                                ),
                                        ))
                                        .class("xcss-table")
                                        .aria_label(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                                                t("实例列表", "Instance list"),
                                            ),
                                        )
                                },
                            )
                            .class("xcss-table-scroll")
                    },
                )
                .class("xcss-content-stack")
                .aria_label(
                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                        t("实例", "Instances"),
                    ),
                ),
        ))
        .class("media-sections"),
)
                })
            }
        }),
        {
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &Pagination,
                {
                    let mut props = component_props_builder(&Pagination)
                        .data(
                            #[allow(unused_braces)]
                            {
                                data
                            },
                        )
                        .cursor(
                            #[allow(unused_braces)]
                            {
                                cursor
                            },
                        )
                        .revision(
                            #[allow(unused_braces)]
                            {
                                revision
                            },
                        )
                        .build();
                    props
                },
            )
        },
    ))
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
    let (data, error) = resource_typed::<User>(Signal::derive(move || p.clone()), revision, 0);
    View::new((
        leptos::html::h1()
            .child(
                #[allow(unused_braces)]
                {
                    IntoRender::into_render({ t("备份详细信息", "Backup details") })
                },
            )
            .class("xcss-visually-hidden"),
        {
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &Failure,
                {
                    let mut props = component_props_builder(&Failure)
                        .error(
                            #[allow(unused_braces)]
                            {
                                error
                            },
                        )
                        .message(
                            #[allow(unused_braces)]
                            {
                                t(
                                    "备份数据暂不可用，请重试。",
                                    "Backup data is temporarily unavailable. Please retry.",
                                )
                            },
                        )
                        .build();
                    props
                },
            )
        },
        {
            #[allow(unreachable_code)]
            #[allow(unused_mut)]
            #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
            component_view(
                #[allow(clippy::needless_borrows_for_generic_args)]
                &For,
                {
                    let mut props = component_props_builder(&For)
                        .each(
                            #[allow(unused_braces)]
                            {
                                { move || data.get().into_iter().collect::<Vec<_>>() }
                            },
                        )
                        .key(
                            #[allow(unused_braces)]
                            {
                                |u| u.id.clone()
                            },
                        )
                        .children(
                            #[allow(unused_braces)]
                            {
                                move |initial| {
                                    let user = Signal::derive(move || {
                                        data.get().unwrap_or_else(|| initial.clone())
                                    });
                                    View::new({
                                        #[allow(unreachable_code)]
                                        #[allow(unused_mut)]
                                        #[allow(
                                            clippy::let_and_return,
                                            clippy::let_unit_value,
                                            clippy::unit_arg
                                        )]
                                        component_view(
                                            #[allow(clippy::needless_borrows_for_generic_args)]
                                            &UserDetails,
                                            {
                                                let mut props =
                                                    component_props_builder(&UserDetails)
                                                        .user(
                                                            #[allow(unused_braces)]
                                                            {
                                                                user
                                                            },
                                                        )
                                                        .revision(
                                                            #[allow(unused_braces)]
                                                            {
                                                                revision
                                                            },
                                                        )
                                                        .data(
                                                            #[allow(unused_braces)]
                                                            {
                                                                data
                                                            },
                                                        )
                                                        .build();
                                                props
                                            },
                                        )
                                    })
                                }
                            },
                        )
                        .build();
                    props
                },
            )
        },
    ))
}
#[component]
fn UserDetails(
    user: Signal<User>,
    revision: RwSignal<u64>,
    data: RwSignal<Option<User>>,
) -> impl IntoView {
    let initial = user.get_untracked();
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None);
    let dialog = RwSignal::new(String::new());
    let name = RwSignal::new(initial.display_name.clone());
    let storage = RwSignal::new(initial.storage_path.clone());
    let quota = RwSignal::new((initial.quota_bytes as f64 / 1_073_741_824.).to_string());
    let id = StoredValue::new(initial.id.clone());
    let saved_quota = RwSignal::new((quota.get_untracked(), initial.quota_bytes));
    let instance = Signal::derive(move || user.with(|u| u.instances.first().cloned()));
    let status = Signal::derive(move || instance.get().map(|i| i.status).unwrap_or("—".into()));
    let terminal = Signal::derive(move || matches!(status.get().as_str(), "cancelled" | "revoked"));
    View::new(
    leptos::html::div()
        .child((
            leptos::html::section()
                .child((
                    leptos::html::h2()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    move || user.with(|u| u.display_name.clone())
                                })
                            },
                        ),
                    leptos::html::dl()
                        .child((
                            leptos::html::dt()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            t("实例名称", "Instance name")
                                        })
                                    },
                                ),
                            leptos::html::dd()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            move || user.with(|u| u.display_name.clone())
                                        })
                                    },
                                ),
                            leptos::html::dt()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            t("实例 ID", "Instance ID")
                                        })
                                    },
                                ),
                            leptos::html::dd()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        leptos::html::code()
                                            .child(
                                                #[allow(unused_braces)]
                                                {
                                                    IntoRender::into_render({
                                                        move || instance.get().map(|i| i.id).unwrap_or("—".into())
                                                    })
                                                },
                                            )
                                    },
                                ),
                            leptos::html::dt()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            t("密码", "Password")
                                        })
                                    },
                                ),
                            leptos::html::dd()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        leptos::html::code()
                                            .child(
                                                #[allow(unused_braces)]
                                                {
                                                    IntoRender::into_render({
                                                        move || {
                                                            instance
                                                                .get()
                                                                .map(|i| i.authorization_code)
                                                                .unwrap_or("—".into())
                                                        }
                                                    })
                                                },
                                            )
                                    },
                                ),
                            leptos::html::dt()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            t("配对状态", "Pairing status")
                                        })
                                    },
                                ),
                            leptos::html::dd()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            move || pairing(&status.get())
                                        })
                                    },
                                ),
                        ))
                        .class("media-detail-list"),
                ))
                .class("xcss-content-panel")
                .aria_label(
                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                        t("配对账户信息", "Pairing account information"),
                    ),
                ),
            leptos::html::section()
                .child((
                    leptos::html::h2()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    t("备份状态", "Backup status")
                                })
                            },
                        ),
                    IntoRender::into_render({
                        move || {
                            user
                                .with(|u| u.instances.clone())
                                .into_iter()
                                .map(|i| {
                                    View::new(
    leptos::html::dl()
        .child((
            leptos::html::dt()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            t("客户端", "Client")
                        })
                    },
                ),
            leptos::html::dd()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            i.name.clone()
                        })
                    },
                ),
            leptos::html::dt()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            t("平台", "Platform")
                        })
                    },
                ),
            leptos::html::dd()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            i.platform.clone()
                        })
                    },
                ),
            leptos::html::dt()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            t("在线状态", "Online status")
                        })
                    },
                ),
            leptos::html::dd()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            if i.online {
                                t("在线", "Online")
                            } else {
                                t("离线", "Offline")
                            }
                        })
                    },
                ),
            leptos::html::dt()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            t("最后在线", "Last seen")
                        })
                    },
                ),
            leptos::html::dd()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            timestamp(&i.last_seen_at.clone())
                        })
                    },
                ),
        ))
        .class("media-detail-list"),
)
                                })
                                .collect_view()
                        }
                    }),
                ))
                .class("xcss-content-panel xcss-content-stack")
                .aria_label(
                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                        t("备份状态", "Backup status"),
                    ),
                ),
            leptos::html::section()
                .child((
                    leptos::html::h2()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    t("实例设置", "Instance settings")
                                })
                            },
                        ),
                    leptos::html::form()
                        .child((
                            {
                                #[allow(unreachable_code)] #[allow(unused_mut)]
                                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                                component_view(
                                    #[allow(clippy::needless_borrows_for_generic_args)]
                                    &Failure,
                                    {
                                        let mut props = component_props_builder(
                                                &Failure,
                                            )
                                            .error(#[allow(unused_braces)] { error })
                                            .message(
                                                #[allow(unused_braces)]
                                                {
                                                    t(
                                                        "未能保存备份实例，请检查名称、路径和配额后重试。",
                                                        "Unable to save the backup instance. Check the name, path, and quota, then retry.",
                                                    )
                                                },
                                            )
                                            .build();
                                        props
                                    },
                                )
                            },
                            leptos::html::label()
                                .child((
                                    IntoRender::into_render({
                                        t("实例名称", "Instance name")
                                    }),
                                    leptos::html::input()
                                        .class("xcss-input")
                                        .name("display_name")
                                        .prop("value", move || name.get())
                                        .required(true)
                                        .readonly(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                            busy.get()),
                                        )
                                        .on(
                                            leptos::ev::input,
                                            move |ev| name.set(event_target_value(&ev)),
                                        ),
                                ))
                                .class("xcss-form-field"),
                            leptos::html::label()
                                .child((
                                    IntoRender::into_render({
                                        t("存储路径", "Storage path")
                                    }),
                                    leptos::html::input()
                                        .class("xcss-input")
                                        .name("storage_path")
                                        .prop("value", move || storage.get())
                                        .required(true)
                                        .readonly(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                            busy.get()),
                                        )
                                        .on(
                                            leptos::ev::input,
                                            move |ev| storage.set(event_target_value(&ev)),
                                        ),
                                ))
                                .class("xcss-form-field"),
                            leptos::html::label()
                                .child((
                                    IntoRender::into_render({
                                        t(
                                            "配额（GiB，0 表示不限）",
                                            "Quota (GiB; 0 means unlimited)",
                                        )
                                    }),
                                    leptos::html::input()
                                        .class("xcss-input")
                                        .name("quota_gib")
                                        .r#type("number")
                                        .min("0")
                                        .step("any")
                                        .prop("value", move || quota.get())
                                        .required(true)
                                        .readonly(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                            busy.get()),
                                        )
                                        .on(
                                            leptos::ev::input,
                                            move |ev| quota.set(event_target_value(&ev)),
                                        ),
                                ))
                                .class("xcss-form-field"),
                            leptos::html::div()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        leptos::html::button()
                                            .child(
                                                #[allow(unused_braces)]
                                                {
                                                    IntoRender::into_render({
                                                        move || {
                                                            if busy.get() {
                                                                t("正在保存…", "Saving…")
                                                            } else {
                                                                t("保存设置", "Save settings")
                                                            }
                                                        }
                                                    })
                                                },
                                            )
                                            .class("xcss-button")
                                            .r#type("submit")
                                            .disabled(
                                                ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                                {
                                                    busy.get() || !valid_name(&name.get())
                                                        || quota_bytes(&quota.get()).is_none()
                                                }),
                                            )
                                    },
                                )
                                .class("xcss-actions"),
                        ))
                        .aria_label(
                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                            {
                                format!(
                                    "{}{}", t("编辑备份实例 ", "Edit backup instance "),
                                    user.with(| u | u.display_name.clone())
                                )
                            }),
                        )
                        .aria_busy(
                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                            busy.get().to_string()),
                        )
                        .on(
                            leptos::ev::submit,
                            move |ev| {
                                ev.prevent_default();
                                let raw = quota.get_untracked();
                                let (saved_raw, saved_bytes) = saved_quota.get_untracked();
                                let amount = if raw.trim() == saved_raw {
                                    Some(saved_bytes)
                                } else {
                                    quota_bytes(&raw)
                                };
                                if let Some(amount) = amount {
                                    let input = UpdateUser {
                                        username: user.with_untracked(|u| u.username.clone()),
                                        display_name: name.get_untracked(),
                                        storage_path: storage.get_untracked(),
                                        quota_bytes: amount,
                                    };
                                    let body = match xcss_web::api::encode_json_body(&input) {
                                        Ok(body) => body,
                                        Err(e) => {
                                            error.set(Some(e));
                                            return;
                                        }
                                    };
                                    mutation_typed::<
                                        User,
                                    >(
                                        format!("/api/v1/admin/users/{}", enc(& id.get_value())),
                                        "PUT",
                                        Some(body),
                                        busy,
                                        error,
                                        move |v| {
                                            saved_quota.set((raw, v.quota_bytes));
                                            data.set(Some(v));
                                            xcss_web::components::notify(
                                                t("实例设置已保存", "Instance settings saved"),
                                            );
                                        },
                                    );
                                }
                            },
                        ),
                ))
                .class("media-card xcss-content-panel")
                .aria_label(
                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                        t("实例设置", "Instance settings"),
                    ),
                ),
            leptos::html::section()
                .child((
                    leptos::html::h2()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    t("实例操作", "Instance actions")
                                })
                            },
                        ),
                    leptos::html::p()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    t(
                                        "实例拥有一个长期客户端密码。服务端加密保存并可查看；更换后旧客户端立即失效并需要重新配对。",
                                        "The instance owns one long-lived client password. The server stores it encrypted and keeps it viewable; changing it invalidates the old client and requires pairing again.",
                                    )
                                })
                            },
                        ),
                    leptos::html::div()
                        .child((
                            {
                                #[allow(unreachable_code)] #[allow(unused_mut)]
                                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                                component_view(
                                    #[allow(clippy::needless_borrows_for_generic_args)]
                                    &Show,
                                    {
                                        let mut props = component_props_builder(
                                                &Show,
                                            )
                                            .when(#[allow(unused_braces)] { move || !terminal.get() })
                                            .children({
                                                ::leptos::children::ToChildren::to_children(move || {
                                                    leptos::html::button()
                                                        .child(
                                                            #[allow(unused_braces)]
                                                            {
                                                                IntoRender::into_render({
                                                                    t("更换密码", "Change password")
                                                                })
                                                            },
                                                        )
                                                        .class("xcss-button")
                                                        .disabled(
                                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                                            busy.get()),
                                                        )
                                                        .on(
                                                            leptos::ev::click,
                                                            move |_| {
                                                                error.set(None);
                                                                dialog.set("rotate".into());
                                                            },
                                                        )
                                                })
                                            })
                                            .build();
                                        props
                                    },
                                )
                            },
                            leptos::html::button()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            move || {
                                                if status.get() == "pending" {
                                                    t("取消配对", "Cancel pairing")
                                                } else if terminal.get() {
                                                    t("删除实例", "Delete instance")
                                                } else {
                                                    t("撤销实例", "Revoke instance")
                                                }
                                            }
                                        })
                                    },
                                )
                                .class("xcss-button")
                                .disabled(
                                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                    busy.get()),
                                )
                                .on(
                                    leptos::ev::click,
                                    move |_| {
                                        error.set(None);
                                        dialog.set("remove".into());
                                    },
                                ),
                        ))
                        .class("xcss-actions"),
                ))
                .class("xcss-content-panel xcss-content-stack")
                .aria_label(
                    ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                        t("实例操作", "Instance actions"),
                    ),
                ),
            IntoRender::into_render({
                move || {
                    (!dialog.get().is_empty())
                        .then(|| {
                            let rotating = dialog.get() == "rotate";
                            let target = instance
                                .get_untracked()
                                .map(|i| i.id)
                                .unwrap_or_default();
                            View::new({
    #[allow(unreachable_code)] #[allow(unused_mut)] #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
    component_view(
        #[allow(clippy::needless_borrows_for_generic_args)]
        &Confirm,
        {
            let mut props = component_props_builder(&Confirm)
                .title(
                    #[allow(unused_braces)]
                    {
                        if rotating {
                            t("更换密码", "Change password")
                        } else if terminal.get_untracked() {
                            t("删除实例", "Delete instance")
                        } else {
                            t("取消或撤销实例", "Cancel or revoke instance")
                        }
                    },
                )
                .description(
                    #[allow(unused_braces)]
                    {
                        if rotating {
                            t(
                                "更换密码会立即撤销当前客户端凭据。客户端必须使用新密码重新配对。",
                                "Changing the password immediately revokes the current client credential. The client must pair again using the new password.",
                            )
                        } else if terminal.get_untracked() {
                            t(
                                "永久删除这条终态且没有备份数据的实例信息。",
                                "Permanently delete this terminal instance entry when it owns no backup data.",
                            )
                        } else {
                            t(
                                "密码和访问令牌将立即失效。终态实例之后可从列表永久删除。",
                                "The password and access token are invalidated immediately. The terminal instance can then be permanently deleted from the list.",
                            )
                        }
                    },
                )
                .error_message(
                    #[allow(unused_braces)]
                    {
                        if rotating {
                            t(
                                "更换未能确认，请刷新核对密码后再操作。",
                                "The change could not be confirmed. Refresh and verify the password before another action.",
                            )
                        } else {
                            t(
                                "操作未能确认，请刷新核对实例状态。",
                                "The operation could not be confirmed. Refresh and check the instance state.",
                            )
                        }
                    },
                )
                .error(#[allow(unused_braces)] { error })
                .busy(#[allow(unused_braces)] { busy })
                .close(
                    #[allow(unused_braces)]
                    { Callback::new(move |()| dialog.set(String::new())) },
                )
                .confirm(
                    #[allow(unused_braces)]
                    {
                        Callback::new(move |()| {
                            let path = format!(
                                "/api/v1/admin/instances/{target}{}", if rotating {
                                "/authorization" } else { "" }
                            );
                            mutation(
                                path,
                                if rotating { "PUT" } else { "DELETE" },
                                None,
                                busy,
                                error,
                                move |_| {
                                    dialog.set(String::new());
                                    revision.update(|v| *v += 1);
                                },
                            );
                        })
                    },
                )
                .build();
            props
        },
    )
})
                        })
                }
            }),
        ))
        .class("xcss-content-stack"),
)
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
    View::new(
    leptos::html::section()
        .child((
            leptos::html::div()
                .child((
                    leptos::html::p()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    move || {
                                        format!(
                                            "{}{}", t("实例：", "Instance: "), instance.get().map(| i
                                            | text(& i, "name")).unwrap_or(t("全部实例",
                                            "All instances"))
                                        )
                                    }
                                })
                            },
                        ),
                    leptos::html::div()
                        .child((
                            leptos::html::label()
                                .child(
                                    #[allow(unused_braces)]
                                    {
                                        IntoRender::into_render({
                                            t("日志日期范围（UTC）", "Log date range (UTC)")
                                        })
                                    },
                                )
                                .r#for("media-log-date-start-year"),
                            leptos::html::div()
                                .child((
                                    IntoRender::into_render({
                                        (!user_id.is_empty())
                                            .then(|| {
                                                View::new(
    leptos::html::button()
        .child(
            #[allow(unused_braces)]
            {
                IntoRender::into_render({
                    t("全部实例", "All instances")
                })
            },
        )
        .class("xcss-button")
        .on(leptos::ev::click, move |_| navigate("logs")),
)
                                            })
                                    }),
                                    leptos::html::button()
                                        .child(
                                            #[allow(unused_braces)]
                                            {
                                                IntoRender::into_render({
                                                    t("刷新日志", "Refresh logs")
                                                })
                                            },
                                        )
                                        .class("xcss-button")
                                        .disabled(
                                            ::leptos::prelude::IntoAttributeValue::into_attribute_value(move ||
                                            !range_valid.get()),
                                        )
                                        .on(
                                            leptos::ev::click,
                                            move |_| {
                                                cursor.set(String::new());
                                                local.update(|v| *v += 1)
                                            },
                                        ),
                                ))
                                .class("xcss-actions"),
                            {
                                #[allow(unreachable_code)] #[allow(unused_mut)]
                                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                                component_view(
                                    #[allow(clippy::needless_borrows_for_generic_args)]
                                    &DateRange,
                                    {
                                        let mut props = component_props_builder(
                                                &DateRange,
                                            )
                                            .id(#[allow(unused_braces)] { "media-log-date" })
                                            .valid(#[allow(unused_braces)] { range_valid })
                                            .value(#[allow(unused_braces)] { range })
                                            .apply(
                                                #[allow(unused_braces)]
                                                {
                                                    Callback::new(move |r: (String, String)| {
                                                        range.set(r.clone());
                                                        applied.set(r);
                                                        cursor.set(String::new());
                                                        local.update(|v| *v += 1);
                                                    })
                                                },
                                            )
                                            .build();
                                        props
                                    },
                                )
                            },
                        ))
                        .class("xcss-log-date-controls"),
                ))
                .class("xcss-content-panel xcss-content-stack"),
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &Failure,
                    {
                        let mut props = component_props_builder(
                                &Failure,
                            )
                            .error(#[allow(unused_braces)] { user_error })
                            .build();
                        props
                    },
                )
            },
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &Failure,
                    {
                        let mut props = component_props_builder(
                                &Failure,
                            )
                            .error(#[allow(unused_braces)] { error })
                            .retry(
                                #[allow(unused_braces)]
                                {
                                    Callback::new(move |()| {
                                        cursor.set(String::new());
                                        local.update(|v| *v += 1);
                                    })
                                },
                            )
                            .message(
                                #[allow(unused_braces)]
                                {
                                    t(
                                        "日志暂不可用，请重试。",
                                        "Logs are temporarily unavailable. Please retry.",
                                    )
                                },
                            )
                            .build();
                        props
                    },
                )
            },
            IntoRender::into_render({
                move || {
                    data
                        .get()
                        .filter(|_| error.get().is_none())
                        .map(|d| {
                            View::new(
    leptos::html::div()
        .child(
            #[allow(unused_braces)]
            {
                leptos::html::table()
                    .child((
                        leptos::html::thead()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    leptos::html::tr()
                                        .child((
                                            leptos::html::th()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        IntoRender::into_render({
                                                            t("时间（UTC）", "Time (UTC)")
                                                        })
                                                    },
                                                ),
                                            leptos::html::th()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        IntoRender::into_render({
                                                            t("操作", "Action")
                                                        })
                                                    },
                                                ),
                                            leptos::html::th()
                                                .child(
                                                    #[allow(unused_braces)]
                                                    {
                                                        IntoRender::into_render({
                                                            t("对象", "Entity")
                                                        })
                                                    },
                                                ),
                                        ))
                                },
                            ),
                        leptos::html::tbody()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    IntoRender::into_render({
                                        rows(&d, "logs")
                                            .into_iter()
                                            .map(|l| {
                                                View::new(
    leptos::html::tr()
        .child((
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        leptos::html::time()
                            .child(
                                #[allow(unused_braces)]
                                {
                                    IntoRender::into_render({
                                        text(&l, "occurred_at")
                                    })
                                },
                            )
                    },
                ),
            leptos::html::td()
                .child((
                    IntoRender::into_render({
                        action_label(&text(&l, "action"))
                    }),
                    " ",
                    leptos::html::code()
                        .child(
                            #[allow(unused_braces)]
                            {
                                IntoRender::into_render({
                                    text(&l, "action")
                                })
                            },
                        ),
                )),
            leptos::html::td()
                .child(
                    #[allow(unused_braces)]
                    {
                        IntoRender::into_render({
                            text(&l, "entity_id")
                        })
                    },
                ),
        )),
)
                                            })
                                            .collect_view()
                                    })
                                },
                            ),
                    ))
                    .class("xcss-table")
                    .aria_label(
                        ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                            t("备份日志记录", "Backup log records"),
                        ),
                    )
            },
        )
        .class("xcss-table-scroll"),
)
                        })
                }
            }),
            {
                #[allow(unreachable_code)] #[allow(unused_mut)]
                #[allow(clippy::let_and_return, clippy::let_unit_value, clippy::unit_arg)]
                component_view(
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    &Pagination,
                    {
                        let mut props = component_props_builder(
                                &Pagination,
                            )
                            .data(#[allow(unused_braces)] { data })
                            .cursor(#[allow(unused_braces)] { cursor })
                            .revision(#[allow(unused_braces)] { local })
                            .label(
                                #[allow(unused_braces)]
                                { t("日志分页", "Log pages") },
                            )
                            .build();
                        props
                    },
                )
            },
        ))
        .class("xcss-content-stack")
        .aria_label(
            ::leptos::prelude::IntoAttributeValue::into_attribute_value(
                t("备份日志", "Backup logs"),
            ),
        ),
)
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

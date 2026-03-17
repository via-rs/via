#![feature(test)]

extern crate test;

use test::Bencher;
use via_router::Router;

macro_rules! benches {
    ($($name:ident => $path:expr),* $(,)?) => {
        $(
            #[bench]
            fn $name(b: &mut Bencher) {
                use std::collections::VecDeque;
                use std::sync::Arc;

                const ROUTES: [&str; 100] = [
                    // ─────────────────────────────────────
                    // Root-level simple pages
                    // ─────────────────────────────────────
                    "/home",
                    "/about",
                    "/contact",
                    "/login",
                    "/signup",
                    "/terms",
                    "/privacy",
                    "/faq",
                    "/sitemap",
                    "/rss",
                    "/invite",

                    // ─────────────────────────────────────
                    // Profile / User
                    // ─────────────────────────────────────
                    "/profile/:user_name",
                    "/user/:user_id",

                    // ─────────────────────────────────────
                    // Settings
                    // ─────────────────────────────────────
                    "/settings",
                    "/settings/account",
                    "/settings/privacy",
                    "/settings/security",

                    // ─────────────────────────────────────
                    // Dashboard
                    // ─────────────────────────────────────
                    "/dashboard",
                    "/dashboard/overview",
                    "/dashboard/stats",
                    "/dashboard/reports",

                    // ─────────────────────────────────────
                    // Search
                    // ─────────────────────────────────────
                    "/search",
                    "/search/results",
                    "/search/:query",

                    // ─────────────────────────────────────
                    // Notifications
                    // ─────────────────────────────────────
                    "/notifications",
                    "/notifications/:notification_id",
                    "/notifications/settings",
                    "/notifications/settings/email",
                    "/notifications/settings/include",

                    // ─────────────────────────────────────
                    // Messages
                    // ─────────────────────────────────────
                    "/messages",
                    "/message/:message_id",
                    "/message/:message_id/reply",

                    // ─────────────────────────────────────
                    // Inbox
                    // ─────────────────────────────────────
                    "/inbox",
                    "/inbox/:conversation_id",
                    "/inbox/:conversation_id/messages",

                    // ─────────────────────────────────────
                    // Posts
                    // ─────────────────────────────────────
                    "/posts",
                    "/post/:post_id",
                    "/post/:post_id/edit",
                    "/post/:post_id/comments",
                    "/post/:post_id/comments/:comment_id",
                    "/post/:post_id/likes",
                    "/post/:post_id/share",

                    // Standalone comments
                    "/comments",
                    "/comment/:comment_id",

                    // ─────────────────────────────────────
                    // Categories
                    // ─────────────────────────────────────
                    "/categories",
                    "/category/:category_id",
                    "/category/:category_id/posts",

                    // ─────────────────────────────────────
                    // Tags
                    // ─────────────────────────────────────
                    "/tags",
                    "/tag/:tag_id",
                    "/tag/:tag_id/posts",

                    // ─────────────────────────────────────
                    // Favorites / Friends
                    // ─────────────────────────────────────
                    "/favorites",
                    "/favorite/:item_id",
                    "/friends",
                    "/friend/:friend_id",

                    // ─────────────────────────────────────
                    // Groups
                    // ─────────────────────────────────────
                    "/groups",
                    "/group/:group_id",
                    "/group/:group_id/members",
                    "/group/:group_id/posts",

                    // ─────────────────────────────────────
                    // Events
                    // ─────────────────────────────────────
                    "/events",
                    "/event/:event_id",
                    "/event/:event_id/rsvp",
                    "/event/:event_id/attendees",

                    // ─────────────────────────────────────
                    // Help
                    // ─────────────────────────────────────
                    "/help",
                    "/help/article/:article_id",

                    // ─────────────────────────────────────
                    // API (deep subtree)
                    // ─────────────────────────────────────
                    "/api/:version/:resource",
                    "/api/:version/:resource/:resource_id",
                    "/api/:version/:resource/:resource_id/edit",
                    "/api/:version/:resource/:resource_id/comments/:comment_id",
                    "/api/:version/:resource/:resource_id/comments/:comment_id/edit",

                    // ─────────────────────────────────────
                    // Checkout
                    // ─────────────────────────────────────
                    "/checkout",
                    "/checkout/cart",
                    "/checkout/payment",
                    "/checkout/confirmation",

                    // ─────────────────────────────────────
                    // Subscriptions
                    // ─────────────────────────────────────
                    "/subscriptions",
                    "/subscription/:subscription_id",
                    "/subscription/:subscription_id/edit",

                    // ─────────────────────────────────────
                    // Billing
                    // ─────────────────────────────────────
                    "/billing",
                    "/billing/history",
                    "/billing/payment-methods",
                    "/billing/invoice/:invoice_id",

                    // ─────────────────────────────────────
                    // Reports
                    // ─────────────────────────────────────
                    "/report/user/:user_id",
                    "/report/post/:post_id",
                    "/report/comment/:comment_id",

                    // ─────────────────────────────────────
                    // Admin (large subtree)
                    // ─────────────────────────────────────
                    "/admin",
                    "/admin/settings",

                    "/admin/users",
                    "/admin/user/:user_id",
                    "/admin/user/:user_id/edit",

                    "/admin/posts",
                    "/admin/post/:post_id",
                    "/admin/post/:post_id/edit",

                    "/admin/comments",
                    "/admin/comment/:comment_id",
                    "/admin/comment/:comment_id/edit",

                    "/admin/categories",
                    "/admin/category/:category_id",
                    "/admin/category/:category_id/edit",

                    "/admin/tags",
                    "/admin/tag/:tag_id",
                    "/admin/tag/:tag_id/edit",
                ];

                let mut router = Router::new();
                let mut spill_check = true;

                for path in test::black_box(ROUTES) {
                    let _ = router.route(path).middleware(Arc::new(()));
                }

                b.iter(|| {
                    let mut deque = VecDeque::new();
                    let mut params = Vec::with_capacity(6);
                    let mut traverse = router.traverse($path);

                    for (route, param) in &mut traverse {
                        deque.extend(route);
                        params.extend(param);
                    }

                    if spill_check && traverse.spilled() {
                        eprintln!("");
                        eprintln!("  warn: allocation required to match {}", $path);
                        eprintln!("");
                    }

                    spill_check = false;
                });
            }
        )*
    };
}

benches! {
    bench_home => "/home",
    bench_about => "/about",
    bench_contact => "/contact",
    bench_login => "/login",
    bench_signup => "/signup",

    bench_profile_user_name => "/profile/a4f90d13a4",
    bench_user_user_id => "/user/7b2e19c4d8",

    bench_settings => "/settings",
    bench_settings_account => "/settings/account",
    bench_settings_privacy => "/settings/privacy",
    bench_settings_security => "/settings/security",

    bench_posts => "/posts",
    bench_post_post_id => "/post/4f90d13a42",
    bench_post_post_id_edit => "/post/4f90d13a42/edit",
    bench_post_post_id_comments => "/post/4f90d13a42/comments",
    bench_post_post_id_comments_comment_id =>
        "/post/4f90d13a42/comments/9c1a7e4b2d",
    bench_post_post_id_likes => "/post/4f90d13a42/likes",
    bench_post_post_id_share => "/post/4f90d13a42/share",

    bench_comments => "/comments",
    bench_comment_comment_id => "/comment/9c1a7e4b2d",

    bench_notifications => "/notifications",
    bench_notifications_notification_id =>
        "/notifications/6e2b4d9a1f",

    bench_messages => "/messages",
    bench_message_message_id => "/message/3d7a1c9b4e",
    bench_message_message_id_reply => "/message/3d7a1c9b4e/reply",

    bench_search => "/search",
    bench_search_results => "/search/results",
    bench_search_query => "/search/8f1d2a7c9b",

    bench_admin => "/admin",
    bench_admin_users => "/admin/users",
    bench_admin_user_user_id => "/admin/user/7b2e19c4d8",
    bench_admin_user_user_id_edit => "/admin/user/7b2e19c4d8/edit",
    bench_admin_posts => "/admin/posts",
    bench_admin_post_post_id => "/admin/post/4f90d13a42",
    bench_admin_post_post_id_edit => "/admin/post/4f90d13a42/edit",
    bench_admin_comments => "/admin/comments",
    bench_admin_comment_comment_id => "/admin/comment/9c1a7e4b2d",
    bench_admin_comment_comment_id_edit =>
        "/admin/comment/9c1a7e4b2d/edit",

    bench_admin_categories => "/admin/categories",
    bench_admin_category_category_id =>
        "/admin/category/1a7d9c4e2f",
    bench_admin_category_category_id_edit =>
        "/admin/category/1a7d9c4e2f/edit",

    bench_admin_tags => "/admin/tags",
    bench_admin_tag_tag_id => "/admin/tag/b4e19d7a2c",
    bench_admin_tag_tag_id_edit =>
        "/admin/tag/b4e19d7a2c/edit",

    bench_admin_settings => "/admin/settings",

    bench_categories => "/categories",
    bench_category_category_id => "/category/1a7d9c4e2f",
    bench_category_category_id_posts =>
        "/category/1a7d9c4e2f/posts",

    bench_tags => "/tags",
    bench_tag_tag_id => "/tag/b4e19d7a2c",
    bench_tag_tag_id_posts =>
        "/tag/b4e19d7a2c/posts",

    bench_favorites => "/favorites",
    bench_favorite_item_id => "/favorite/d2a7c9e4f1",

    bench_friends => "/friends",
    bench_friend_friend_id => "/friend/c9e4b1a7d2",

    bench_groups => "/groups",
    bench_group_group_id => "/group/5e1a9d4c7b",
    bench_group_group_id_members =>
        "/group/5e1a9d4c7b/members",
    bench_group_group_id_posts =>
        "/group/5e1a9d4c7b/posts",

    bench_events => "/events",
    bench_event_event_id => "/event/2c7d1a9e4b",
    bench_event_event_id_rsvp =>
        "/event/2c7d1a9e4b/rsvp",
    bench_event_event_id_attendees =>
        "/event/2c7d1a9e4b/attendees",

    bench_help => "/help",
    bench_help_article_article_id =>
        "/help/article/e4b1c9a7d2",

    bench_terms => "/terms",
    bench_privacy => "/privacy",
    bench_faq => "/faq",
    bench_sitemap => "/sitemap",
    bench_rss => "/rss",

    bench_api_version_resource =>
        "/api/v1/products",
    bench_api_version_resource_resource_id =>
        "/api/v1/products/4f90d13a42",
    bench_api_version_resource_resource_id_edit =>
        "/api/v1/products/4f90d13a42/edit",
    bench_api_version_resource_resource_id_comments_comment_id =>
        "/api/v1/products/4f90d13a42/comments/9c1a7e4b2d",
    bench_api_version_resource_resource_id_comments_comment_id_edit =>
        "/api/v1/products/4f90d13a42/comments/9c1a7e4b2d/edit",

    bench_checkout => "/checkout",
    bench_checkout_cart => "/checkout/cart",
    bench_checkout_payment => "/checkout/payment",
    bench_checkout_confirmation => "/checkout/confirmation",

    bench_dashboard => "/dashboard",
    bench_dashboard_overview => "/dashboard/overview",
    bench_dashboard_stats => "/dashboard/stats",
    bench_dashboard_reports => "/dashboard/reports",

    bench_notifications_settings => "/notifications/settings",
    bench_notifications_settings_email =>
        "/notifications/settings/email",
    bench_notifications_settings_include =>
        "/notifications/settings/include",

    bench_inbox => "/inbox",
    bench_inbox_conversation_id =>
        "/inbox/a9d4c7e1b2",
    bench_inbox_conversation_id_messages =>
        "/inbox/a9d4c7e1b2/messages",

    bench_subscriptions => "/subscriptions",
    bench_subscription_subscription_id =>
        "/subscription/7e1a9d4c2b",
    bench_subscription_subscription_id_edit =>
        "/subscription/7e1a9d4c2b/edit",

    bench_billing => "/billing",
    bench_billing_history => "/billing/history",
    bench_billing_payment_methods =>
        "/billing/payment-methods",
    bench_billing_invoice_invoice_id =>
        "/billing/invoice/1c9e4b7a2d",

    bench_report_user_user_id =>
        "/report/user/7b2e19c4d8",
    bench_report_post_post_id =>
        "/report/post/4f90d13a42",
    bench_report_comment_comment_id =>
        "/report/comment/9c1a7e4b2d",

    bench_invite => "/invite",
}

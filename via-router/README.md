# Via Router

The router that inspired the framework.

## Benchmarks

To run the benchmarks, execute the following command:

```
cargo +nightly bench --features benches -- --no-capture
```

### Environment

**Hardware / OS:**
Apple MacBook Pro (14-inch, M2 Pro, 2023)
Fedora Linux Asahi Remix 42

### Results

---

| Route | ns/iter | +/- |
|-------|--------:|----:|
| /home | 71.32 | 0.49 |
| /about | 79.55 | 1.45 |
| /contact | 79.99 | 0.91 |
| /login | 79.78 | 0.55 |
| /signup | 78.73 | 0.76 |
| /profile/:user_name | 104.43 | 0.59 |
| /user/:user_id | 97.14 | 0.72 |
| /settings | 74.86 | 0.87 |
| /settings/account | 96.54 | 0.88 |
| /settings/privacy | 97.20 | 0.82 |
| /settings/security | 93.51 | 0.80 |
| /posts | 78.70 | 1.31 |
| /post/:post_id | 98.49 | 0.94 |
| /post/:post_id/edit | 114.36 | 1.03 |
| /post/:post_id/comments | 117.43 | 1.34 |
| /post/:post_id/comments/:comment_id | 145.95 | 1.65 |
| /post/:post_id/likes | 118.27 | 0.65 |
| /post/:post_id/share | 118.74 | 1.41 |
| /comments | 74.75 | 1.02 |
| /comment/:comment_id | 104.49 | 0.99 |
| /notifications | 70.15 | 1.18 |
| /notifications/:notification_id | 97.77 | 0.79 |
| :package: /notifications/settings | 137.21 | 1.01 |
| :package: /notifications/settings/email | 156.31 | 1.58 |
| :package: /notifications/settings/include | 156.49 | 0.91 |
| /messages | 74.14 | 1.16 |
| /message/:message_id | 104.51 | 2.21 |
| /message/:message_id/reply | 121.40 | 0.73 |
| /search | 78.63 | 1.64 |
| /search/:query | 107.43 | 1.25 |
| :package: /search/results | 144.72 | 1.04 |
| /admin | 82.91 | 1.69 |
| /admin/users | 103.71 | 1.78 |
| /admin/user/:user_id | 130.83 | 0.90 |
| /admin/user/:user_id/edit | 147.13 | 0.83 |
| /admin/posts | 102.45 | 1.35 |
| /admin/post/:post_id | 130.44 | 1.93 |
| /admin/post/:post_id/edit | 146.06 | 2.43 |
| /admin/comments | 108.59 | 1.69 |
| /admin/comment/:comment_id | 126.17 | 2.37 |
| /admin/comment/:comment_id/edit | 141.28 | 1.95 |
| /admin/categories | 102.65 | 1.69 |
| /admin/category/:category_id | 132.39 | 2.14 |
| /admin/category/:category_id/edit | 146.91 | 3.23 |
| /admin/tags | 103.96 | 0.71 |
| /admin/tag/:tag_id | 122.71 | 0.65 |
| /admin/tag/:tag_id/edit | 138.01 | 1.68 |
| /admin/settings | 107.57 | 1.21 |
| /categories | 66.87 | 0.57 |
| /category/:category_id | 97.88 | 1.33 |
| /category/:category_id/posts | 116.15 | 0.57 |
| /tags | 72.40 | 1.31 |
| /tag/:tag_id | 95.74 | 0.67 |
| /tag/:tag_id/posts | 113.55 | 2.34 |
| /favorites | 67.78 | 0.38 |
| /favorite/:item_id | 98.19 | 0.91 |
| /friends | 80.52 | 0.72 |
| /friend/:friend_id | 101.59 | 0.75 |
| /groups | 77.17 | 1.39 |
| /group/:group_id | 103.40 | 0.95 |
| /group/:group_id/members | 122.68 | 0.94 |
| /group/:group_id/posts | 121.14 | 0.71 |
| /events | 78.09 | 1.61 |
| /event/:event_id | 103.28 | 0.85 |
| /event/:event_id/rsvp | 120.50 | 0.74 |
| /event/:event_id/attendees | 121.91 | 0.93 |
| /help | 72.01 | 0.68 |
| /help/article/:article_id | 113.54 | 1.64 |
| /terms | 79.79 | 0.79 |
| /privacy | 79.75 | 1.37 |
| /faq | 71.12 | 0.58 |
| /sitemap | 80.61 | 1.28 |
| /rss | 71.18 | 1.20 |
| /api/:version/:resource | 116.16 | 2.03 |
| /api/:version/:resource/:resource_id | 142.79 | 1.27 |
| /api/:version/:resource/:resource_id/edit | 159.09 | 0.83 |
| /api/:version/:resource/:resource_id/comments/:comment_id | 183.32 | 1.94 |
| /api/:version/:resource/:resource_id/comments/:comment_id/edit | 199.54 | 1.24 |
| /checkout | 74.57 | 1.15 |
| /checkout/cart | 91.21 | 0.37 |
| /checkout/payment | 93.02 | 1.35 |
| /checkout/confirmation | 93.95 | 0.87 |
| /dashboard | 69.05 | 0.29 |
| /dashboard/overview | 86.36 | 1.05 |
| /dashboard/stats | 87.98 | 1.16 |
| /dashboard/reports | 87.61 | 1.30 |
| /inbox | 79.50 | 1.06 |
| /inbox/:conversation_id | 107.14 | 0.86 |
| /inbox/:conversation_id/messages | 124.54 | 1.50 |
| /subscriptions | 69.55 | 0.39 |
| /subscription/:subscription_id | 91.95 | 0.62 |
| /subscription/:subscription_id/edit | 109.13 | 1.64 |
| /billing | 80.54 | 1.23 |
| /billing/history | 101.23 | 0.94 |
| /billing/payment-methods | 101.28 | 0.75 |
| /billing/invoice/:invoice_id | 126.20 | 1.33 |
| /report/user/:user_id | 115.19 | 0.53 |
| /report/post/:post_id | 116.16 | 0.64 |
| /report/comment/:comment_id | 116.29 | 2.30 |
| /invite | 77.77 | 1.65 |

### Key

| Emoji     | Definition                                   |
|:---------:|----------------------------------------------|
| :package: | dynamic allocation required during traversal |

# 百度学术新版 JSON 搜索接口调研与实现

- **API 类型**: 未公开站内 JSON 接口(实验性;非正式 API,无稳定承诺)
- **端点**: `GET https://xueshu.baidu.com/search/api/search`
- **认证**: 无 Cookie / 无账号;需特殊请求头(见下)
- **能力**: search
- **实现**: `src/sources/xueshu.rs`(以代码为准)

## 必需请求头

- `Acs-Token: parisInstance is not ready` — 非随机猜测值:官网前端反风险 SDK(paris)未初始化时发送的静态回退字符串(来源:前端主包 Cnu8PP3d.js),后端当前接受(2026-07 实测)。失效表现为业务码 7350001。
- 必须带非默认 User-Agent(实现发送项目 UA `fastpaper-cli/<ver> (+repo url)`);curl 默认 UA 会触发验证码(7350001)。

## 请求参数

| 参数 | 说明 |
|---|---|
| `wd` | 搜索词,URL 编码(必需) |
| `pn` | 结果偏移;首页 `0`,翻页 `pn += 10` |
| `skipStrategy` | 固定传 `0` |

每页固定 10 条。实现串行翻页,凑够 max_results、遇空页或出错即停,最后截断到 max_results。另有 `sort` / `filter` / `slang` 参数(前端代码确认存在,未逐值验证,实现未用)。

## 响应结构

```text
status: { code, msg, sLogid }
data.paper: { dispnum(总数), searchResult, skipStrategy, paperList[] }
paperList[]: paperId, title, abstract, publishYear, publishTime, cited, doi,
             keyword, type, langType,
             authors[]{ name, showName, affiliate },
             publishInfo{ publisher, journalName, journalId },
             sourceList[]{ url, anchor, domain, type, size },
             saveList[]{ url, anchor, type, size }
```

字段可能为 `null`、空串或整体缺失,解析需容错。`title` / `abstract` 可能含搜索高亮标签(如 `<em>大语言模型</em>`)。

## 字段映射(→ Paper)

| Paper 字段 | 来源 |
|---|---|
| `id` | `paperId` |
| `title` | `title` 去 `<em>` 等标签并解码 HTML 实体(abstract 同) |
| `authors` | `authors[].showName`,为空回退 `name`,全空则过滤 |
| `abstract` | `abstract`(空串转 None) |
| `year` | `publishYear`(0 视为无) |
| `doi` | `doi`(空串转 None) |
| `url` | `https://xueshu.baidu.com/usercenter/paper/show?paperid={paperId}&site=xueshu_se` |
| `pdf_url` | `saveList[]` 中首个含 `.pdf` 的 http 链接;无则 None(saveList 混有第三方落地页) |
| `venue` | `publishInfo.journalName`,为空回退 `publisher` |
| `citations` | `cited` |
| `source` | 固定 `xueshu` |

## 错误处理

| 情况 | 处理 |
|---|---|
| HTTP 200 且 `status.code=0` | 正常解析(`data` 缺失视为空结果) |
| HTTP 206 或 `status.code=7350001` | 需交互式验证码:报明确错误,不自动处理、不重试 |
| HTTP 403 | 被百度安全策略拦截 |
| HTTP 429 | 频率限制,提示稍后重试 |
| 其他非零 `status.code` | 报错并带上 `msg` 与 `sLogid` |
| `paperList` 为空 | 正常结束分页 |

## 最小复现

```bash
curl --compressed -A 'fastpaper-cli/0.1' -H 'Acs-Token: parisInstance is not ready' 'https://xueshu.baidu.com/search/api/search?wd=attention%20mechanism&pn=0&skipStrategy=0' | jq '{status, count: (.data.paper.paperList | length)}'
```

预期:`status.code=0`、`count=10`。

## 边界须知

- 未公开接口,百度无稳定承诺;定位为实验性数据源,参数与响应结构可能随前端发版变化,须能随时停用。
- `Acs-Token` 回退值可能随时被后端停止接受,表现为 7350001。
- 风控按查询重复度与 IP 频率分级;保持串行低频请求,避免并发轰炸。
- 绝不自动过验证码、获取 `svcp_stk` 或逆向生成风险 Token;触发验证码即报错停止。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` / `--offset` | 本地切片(接口按 `pn` 每页 10 条串行翻页) |
| 其余全部 | 不支持。非官方接口,不做超出必要的依赖 |

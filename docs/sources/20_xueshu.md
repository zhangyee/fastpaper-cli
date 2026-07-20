# 百度学术新版 JSON 搜索接口调研与实现

> 调研日期：2026-07-20（Asia/Shanghai）  
> 调研对象：百度学术新版搜索页  
> 结论状态：已在 Chrome 与独立命令行 HTTP 客户端中交叉验证

## 1. 结论摘要

百度学术旧版 `/s?wd=...` 页面不适合直接由 CLI 抓取：在本次测试环境中，普通 HTTP 请求稳定返回 `403 百度安全验证`。

新版页面实际调用一个 JSON 搜索接口：

```text
GET https://xueshu.baidu.com/search/api/search
```

经官网前端代码和独立请求对照验证，该接口当前可通过以下最小调用方式访问：

```http
GET /search/api/search?wd=attention%20mechanism&pn=0&skipStrategy=0 HTTP/1.1
Host: xueshu.baidu.com
Acs-Token: parisInstance is not ready
```

其中 `Acs-Token: parisInstance is not ready` 不是随机猜测值，而是百度学术新版前端在反风险 SDK 尚未初始化时使用的静态回退值。2026-07-20 的实测表明，后端当前接受该回退值，并返回完整搜索结果。

这是一条未公开的站内接口调用路径，不是百度承诺稳定的正式 API。适合实现为实验性数据源，但必须准备好在接口策略变化时快速停用或更新。

## 2. 新旧版调用链对比

### 2.1 旧版

旧版搜索地址：

```text
https://xueshu.baidu.com/s?wd={query}&pn={offset}&tn=SE_baiduxueshu_c1gjeupa&ie=utf-8
```

浏览器中可以正常显示服务端渲染的 HTML，但独立 HTTP 客户端的结果是：

- 直接请求：HTTP `403`；
- 页面标题：`百度安全验证`；
- 先访问首页并携带匿名 Cookie 后再请求：仍为 HTTP `403`。

因此，旧版 HTML 解析器本身虽然容易实现，但传输层不可稳定使用，不建议作为 CLI 接入方案。

### 2.2 新版

新版搜索页面：

```text
https://xueshu.baidu.com/ndscholar/browse/search?wd={query}
```

Chrome 中执行搜索后，页面实际发起：

```text
https://xueshu.baidu.com/search/api/search?wd=attention+mechanism&skipStrategy=0
```

切换至第二页后，页面发起：

```text
https://xueshu.baidu.com/search/api/search?wd=attention+mechanism&pn=10&skipStrategy=0
```

新版使用 JSON 返回结果，不需要解析整个搜索结果 HTML。

## 3. 官网前端代码证据

本次检查的公开前端主包：

- [百度学术新版前端主包 Cnu8PP3d.js](https://wkstatic.bdimg.com/static/ndpcwenku/static/ndscholar/scholar_static/Cnu8PP3d.js)
- [百度学术新版搜索页面](https://xueshu.baidu.com/ndscholar/browse/search?wd=attention+mechanism)

前端主包中可以确认以下事实。

### 3.1 搜索 API 常量

前端将搜索接口定义为：

```text
SEARCH = "/search/api/search"
```

搜索函数使用 GET 查询参数调用该接口。

### 3.2 搜索接口被列为受保护接口

`/search/api/search` 位于前端的受保护 URL 列表中。请求封装会为这些接口添加：

```http
Acs-Token: {token}
```

### 3.3 SDK 未就绪时的回退值

前端调用反风险 SDK 生成 Token。如果 SDK 实例尚未出现，前端返回以下字符串，并继续将其作为 `Acs-Token` 发送：

```text
parisInstance is not ready
```

如果 Token 获取超时，前端还定义了另一回退错误码字符串，但本次实现没有依赖该路径。

### 3.4 验证码分支

当响应业务码为 `7350001` 时，前端会启动交互式验证码流程；验证成功后将 `svcp_stk` 加入 URL 并重试。

CLI 实现不应自动处理或规避交互式验证码。如果静态回退值失效并触发该业务码，应当返回清晰错误并停止请求。

## 4. 对照实验

所有命令行实验均使用独立 `curl` 进程，不读取或复用 Chrome Cookie。

| 实验 | Cookie | Referer | 自定义 `Acs-Token` | 结果 |
|---|---:|---:|---:|---|
| 直接请求新版 API | 无 | 无 | 无 | HTTP `206`，88 字节，`status.code=7350001` |
| 先访问首页再请求 | 匿名 Cookie | 搜索页 | 无 | HTTP `206`，仍为 `7350001` |
| 使用官网 SDK 回退值 | 无 | 无 | `parisInstance is not ready` | HTTP `200`，`status.code=0`，返回 10 条 |
| 请求 `pn=10` | 无 | 无 | 同上 | HTTP `200`，返回不同的第二页结果 |
| 中文关键词“大语言模型” | 无 | 无 | 同上 | HTTP `200`，返回完整中文结果和元数据 |

一次英文第一页实测：

```text
HTTP=200
BYTES=13627
Content-Type=application/json; charset=utf-8
paperList.length=10
```

英文第二页实测：

```text
HTTP=200
BYTES=15689
paperList.length=10
dispnum=27846
pn=10
```

中文搜索实测：

```text
query=大语言模型
HTTP=200
BYTES=10569
paperList.length=10
dispnum=249428
```

返回结果中的第一篇论文包含：

- `paperId`；
- 中文标题；
- 4 位作者及机构；
- 摘要；
- 发表年份；
- 被引次数；
- DOI；
- 期刊名称；
- 5 个来源链接；
- 1 个可获取来源。

不同请求返回不同的 `status.sLogid`，第二页也返回不同的 `paperId`，因此这些结果不是误读本地缓存或验证码响应。

## 5. 最小可复现请求

```bash
curl --compressed \
  -H 'Acs-Token: parisInstance is not ready' \
  'https://xueshu.baidu.com/search/api/search?wd=attention%20mechanism&pn=0&skipStrategy=0'
```

检查响应：

```bash
curl --compressed \
  -H 'Acs-Token: parisInstance is not ready' \
  'https://xueshu.baidu.com/search/api/search?wd=attention%20mechanism&pn=0&skipStrategy=0' \
  | jq '{status, count:(.data.paper.paperList | length)}'
```

预期结构：

```json
{
  "status": {
    "code": 0,
    "msg": "Success",
    "sLogid": "..."
  },
  "count": 10
}
```

## 6. 请求参数

| 参数 | 含义 | 验证状态 |
|---|---|---|
| `wd` | 搜索词，URL 编码 | 已验证，必需 |
| `pn` | 结果偏移量；第一页为 `0` 或省略，第二页为 `10` | 已验证 |
| `skipStrategy` | 前端搜索策略状态，当前默认 `0` | 已验证 |
| `sort` | 排序条件，由新版路由透传 | 前端代码确认，未逐值测试 |
| `filter` | 年份、领域、类型等组合过滤条件 | 前端代码确认，未逐值测试 |
| `slang` | 语言筛选 | 前端代码确认，未逐值测试 |

每页固定返回 10 条。实现 `max_results` 时应按 `pn += 10` 翻页，并在满足数量、结果为空或出现错误时停止。

## 7. JSON 响应结构

顶层结构：

```text
response
├── status
│   ├── code
│   ├── msg
│   └── sLogid
└── data
    ├── paper
    │   ├── dispnum
    │   ├── searchResult
    │   ├── skipStrategy
    │   └── paperList[]
    ├── authors
    └── journal
```

单篇论文的主要字段：

```text
paperList[]
├── paperId
├── title
├── abstract
├── publishYear
├── publishTime
├── cited
├── doi
├── keyword
├── type
├── langType
├── authors[]
│   ├── name
│   ├── showName
│   └── affiliate
├── publishInfo
│   ├── publisher
│   ├── journalName
│   └── journalId
├── sourceList[]
│   ├── url
│   ├── anchor
│   ├── domain
│   ├── type
│   └── size
└── saveList[]
    ├── url
    ├── anchor
    ├── type
    └── size
```

注意：`title` 可能包含搜索高亮标签，例如：

```html
ChatModeler:基于<em>大语言模型</em>的人机协作迭代式需求获取和建模方法
```

输出前应将其转换为纯文本。

## 8. fastpaper 字段映射建议

| fastpaper `Paper` 字段 | 百度学术字段或生成方式 |
|---|---|
| `id` | `paperId` |
| `title` | `title` 去除 `<em>` 等 HTML 标签 |
| `authors` | `authors[].showName`，为空时回退 `authors[].name` |
| `abstract` | `abstract` |
| `year` | `publishYear` |
| `doi` | `doi`，空字符串转 `None` |
| `url` | `https://xueshu.baidu.com/usercenter/paper/show?paperid={paperId}&site=xueshu_se` |
| `pdf_url` | `saveList` 中首个可信下载 URL；不存在则 `None` |
| `venue` | `publishInfo.journalName`，为空时回退 `publishInfo.publisher` |
| `citations` | `cited` |
| `source` | 固定为 `xueshu` |

`sourceList` 与作者机构等额外数据当前无法完全放入 fastpaper 的基础 `Paper` 结构，可先忽略，或在以后扩充通用元数据字段。

## 9. Rust/ureq 实现轮廓

下面代码展示请求层的核心写法，省略了完整数据结构和错误枚举：

```rust
const XUESHU_BASE_URL: &str = "https://xueshu.baidu.com";
const ACS_TOKEN_FALLBACK: &str = "parisInstance is not ready";

fn search_page(
    base_url: &str,
    query: &str,
    offset: u32,
) -> Result<XueshuSearchResponse, String> {
    let encoded = encode_query(query);
    let url = format!(
        "{}/search/api/search?wd={}&pn={}&skipStrategy=0",
        base_url, encoded, offset
    );

    let response = ureq::get(&url)
        .header("Acs-Token", ACS_TOKEN_FALLBACK)
        .header("Accept", "application/json, text/plain, */*")
        .call()
        .map_err(|error| format!("百度学术请求失败: {error}"))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|error| format!("读取百度学术响应失败: {error}"))?;

    serde_json::from_str(&body)
        .map_err(|error| format!("解析百度学术响应失败: {error}"))
}
```

建议的数据结构：

```rust
#[derive(Debug, serde::Deserialize)]
struct XueshuSearchResponse {
    status: XueshuStatus,
    data: Option<XueshuData>,
}

#[derive(Debug, serde::Deserialize)]
struct XueshuStatus {
    code: i64,
    msg: String,
    #[serde(rename = "sLogid")]
    slogid: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct XueshuData {
    paper: XueshuPaperResults,
}

#[derive(Debug, serde::Deserialize)]
struct XueshuPaperResults {
    dispnum: Option<u64>,
    #[serde(rename = "skipStrategy")]
    skip_strategy: Option<u32>,
    #[serde(rename = "paperList", default)]
    paper_list: Vec<XueshuPaper>,
}

#[derive(Debug, serde::Deserialize)]
struct XueshuPaper {
    #[serde(rename = "paperId")]
    paper_id: String,
    title: String,
    #[serde(default)]
    abstract_text: String,
    #[serde(rename = "publishYear")]
    publish_year: Option<u16>,
    cited: Option<u32>,
    #[serde(default)]
    doi: String,
    #[serde(default)]
    authors: Vec<XueshuAuthor>,
    #[serde(rename = "publishInfo")]
    publish_info: Option<XueshuPublishInfo>,
    #[serde(rename = "sourceList", default)]
    source_list: Vec<XueshuSource>,
    #[serde(rename = "saveList", default)]
    save_list: Vec<XueshuSource>,
}
```

字段类型应以保存下来的真实 fixture 为准；百度可能对缺失字段返回 `null`、空字符串或省略字段，因此可选字段需要使用 `Option<T>` 或 `#[serde(default)]`。

## 10. 搜索循环

```text
papers = []
offset = 0

while papers.len < max_results:
    response = search_page(query, offset)

    if response.status.code == 7350001:
        return error("百度学术要求交互式验证码")

    if response.status.code != 0:
        return error(response.status.msg)

    page = response.data.paper.paperList
    if page is empty:
        break

    append converted papers
    offset += 10

return papers[..max_results]
```

建议默认限制较低，并避免并发轰炸该接口。搜索结果可以按查询参数短期缓存，以减少重复访问。

## 11. 错误处理

实现至少应区分：

| 情况 | 处理建议 |
|---|---|
| HTTP `200` 且 `status.code=0` | 正常解析 |
| HTTP `206` 或 `status.code=7350001` | 返回“需要百度交互式验证码”错误，不自动处理 |
| HTTP `403` | 返回“请求被百度安全策略拦截” |
| HTTP `429` | 返回频率限制错误，可提示稍后重试 |
| JSON 结构变化 | 返回解析错误，并记录 `status.sLogid` |
| `paperList` 为空 | 正常结束分页 |

不建议对验证码错误进行高频自动重试，这只会增加后续封禁概率。

## 12. 测试建议

### 12.1 Fixture 单元测试

保存一份脱敏后的真实 JSON 响应，例如：

```text
tests/fixtures/xueshu_search.json
```

覆盖以下行为：

- 返回 10 篇论文；
- 正确读取 `paperId`；
- 去除标题中的 `<em>`；
- 解析中文和英文作者；
- 解析年份、DOI、期刊和引用数；
- 从 `saveList` 选择下载 URL；
- 空字符串和 `null` 不导致解析失败；
- `status.code=7350001` 返回明确错误。

### 12.2 mockito 请求测试

验证：

- 请求路径为 `/search/api/search`；
- 存在 `wd`、`pn` 和 `skipStrategy=0`；
- 请求包含 `Acs-Token`；
- 第二页使用 `pn=10`；
- 达到 `max_results` 后停止请求。

### 12.3 真实集成测试

真实接口测试应标记为 `#[ignore]`，避免日常 CI 持续访问百度：

```text
1. 请求一个低风险关键词；
2. 断言 HTTP 成功；
3. 断言 status.code == 0；
4. 断言 paperList 非空；
5. 失败时打印 status.code、msg 和 sLogid。
```

## 13. 稳定性与合规边界

### 已确认

- 新版页面真实使用 `/search/api/search`；
- 搜索请求需要 `Acs-Token` 请求头；
- 官网代码包含静态 SDK 未就绪回退值；
- 当前后端接受该回退值；
- 无 Cookie、无 Referer的独立客户端可成功搜索；
- 英文、中文和第二页请求均成功。

### 未获得保证

- 百度没有将此接口发布为正式公共 API；
- 回退值未来可能不再被后端接受；
- 不同地区、IP、频率或网络环境可能采用不同风险策略；
- 返回结构和参数名称可能随新版前端发布而变化；
- 接口的使用条款、频率上限和长期可用性没有公开 SLA。

如果接口开始要求交互式验证码，应停止自动访问，不应尝试自动获取 `svcp_stk`、模拟验证码通过或逆向生成有效风险 Token。

## 14. 推荐落地策略

1. 使用新版 JSON 接口，不实现旧版 HTML 抓取。
2. 将数据源标记为 `experimental`。
3. 将基础 URL做成环境变量，例如 `FASTPAPER_XUESHU_URL`。
4. 将 `Acs-Token` 回退值集中定义并写明来源，便于更新。
5. 默认串行请求，每页 10 条，设置合理超时。
6. 对相同查询做短期缓存。
7. 保留 fixture 单元测试和一个手动执行的真实健康检查。
8. 当 `7350001`、`403` 或解析错误比例上升时，停止使用并重新审查官网调用链。

## 15. 最终判断

与旧版 `/s` HTML 抓取相比，新版 JSON 接口更适合 fastpaper 一类 CLI：响应结构清晰、字段完整、中文支持良好，而且分页行为明确。

当前实现成本较低，主要工作是 serde 数据结构、字段映射、分页与错误处理。真正的不确定性不是代码，而是未公开接口和反风险回退行为能维持多久。因此，合理定位是“当前已验证可用的实验性数据源”，而不是“有稳定承诺的正式 API”。

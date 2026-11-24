import type { FileAccessPolicy, FolderGrant } from "@/lib/use-bridge";
import type { Language, Translate } from "@/lib/i18n";

export const accessOperations = ["search", "read", "write"] as const;

export function folderGrantEnabled(policy: FileAccessPolicy, operation: keyof FolderGrant, alias: string) {
  return policy.folder_grants?.[alias]?.[operation] ?? true;
}

export function hasFileGrant(policy: FileAccessPolicy | null) {
  return !!policy && accessOperations.some(operation => policy[operation] === "all_local"
    || (policy[operation] === "folders" && Object.keys(policy.roots).some(alias => folderGrantEnabled(policy, operation, alias))));
}

export function accessPolicyKey(policy: FileAccessPolicy) {
  // Snapshot events and IPC replies can serialize object fields in different orders.
  const roots = Object.keys(policy.roots).sort().map(alias => [alias, policy.roots[alias]]);
  const grants = policy.folder_grants;
  const folderGrants = grants && Object.keys(grants).sort().map(alias => {
    const { search, read, write } = grants[alias];
    return [alias, search, read, write];
  });
  const fields = [policy.mode, policy.search, policy.read, policy.write, roots, folderGrants];
  return JSON.stringify(policy.disks ? [...fields, [...policy.disks].sort()] : fields);
}

export const agentCredentialIssue = (t: Translate) => ({
  title: t("生成 Muse 专用令牌？"),
  description: t("连接开启时，持有此令牌的 Muse 可以按已保存的搜索、读取与写入权限访问本机文件。令牌应填入 Muse 安全卡片。"),
  action: t("生成专用令牌"),
  busy: t("正在生成 Muse 专用令牌"),
  done: t("Muse 专用令牌已生成"),
});

export function agentTrial(t: Translate, policy: FileAccessPolicy | null, preferredAlias?: string | null) {
  const operation = (["read", "search"] as const).find(key => policy?.[key] === "all_local"
    || (policy?.[key] === "folders" && Object.keys(policy.roots).some(alias => folderGrantEnabled(policy, key, alias))));
  const aliases = policy && operation && policy[operation] === "folders"
    ? Object.keys(policy.roots).filter(alias => folderGrantEnabled(policy, operation, alias)).sort()
    : [];
  const alias = preferredAlias && aliases.includes(preferredAlias) ? preferredAlias : aliases[0] ?? "";
  const folder = alias ? t("目录 {0}", JSON.stringify(alias)) : t("已授权的磁盘");

  if (operation === "read") return {
    aliases, alias, operation,
    prepare: alias ? t("请在目录「{0}」准备一份无敏感内容的 hello.txt。", alias) : t("请准备一份已授权、可以安全试读的文件。"),
    instruction: alias
      ? t("在 {0} 读取 hello.txt。若样本尚未准备，请先问我试读文件路径。", folder)
      : t("先问我要试读的文件路径，再在该路径所对应的授权目录内读取文件。"),
    confirm: t("我已在 Muse 读到文件"),
  };
  if (operation === "search") return {
    aliases, alias, operation,
    prepare: alias ? t("请选一个目录「{0}」里已知的文件名。", alias) : t("请选一个已授权磁盘里已知的文件名。"),
    instruction: t("先问我要找的文件名，再在{0}中搜索，返回实际匹配的路径。若索引仍在建立，请等候并重试。", folder),
    confirm: t("我已在 Muse 找到文件"),
  };
  return {
    aliases, alias, operation,
    prepare: t("你可以在 Muse 核对已授权的目录和权限。"),
    instruction: t("调用 GET /agent/v1/roots，告诉我各操作已授权的目录。确认授权即可，无需创建文件。"),
    confirm: t("我已在 Muse 核对授权"),
  };
}

export function connectorBrief(language: Language, origin: string, trialInstruction: string) {
  if (language === "en") return `Set up a Custom Connector named “Muse Local Files”. Its HTTPS base URL is ${origin} and its API prefix is /agent/v1.
Use a security card to collect the dedicated agent-token. Send Authorization: Bearer <token> on each request. Store it separately and use it only for requests to ${origin}/agent/v1/. Show only the results in the conversation.

The API returns JSON. Send Content-Type: application/json for POST requests and URL-encode GET query parameters. Use the authorized alias as root and the relative path within it as path.
- GET /agent/v1/roots returns roots.search, roots.read, and roots.write, each listing the aliases authorized for that operation.
- GET /agent/v1/locations/desktop returns root, path, and the absolute_path of the Windows Desktop. Use this to locate the Desktop if it is within the authorized scope.
- POST /agent/v1/search accepts {"name":"part of the file name","limit":30}. Optional fields: root, extensions, modified_since_ms, modified_before_ms, and cursor. If you know the drive letter, pass its root alias first. Results include hits, complete, scanning, backend, scopes, and optional next_cursor and retry_after_ms. Each hit has root, path, kind, size, and modified_ms. When scanning is true, show the current results and scopes still being indexed, wait for retry_after_ms, and try again. If indexing is unfinished or complete is false, say that coverage is incomplete. To paginate, pass next_cursor as cursor and keep the other query parameters the same.
- If search returns HTTP 409 with search_index_cleared, ask the user to open File Access in MuseLink and select Rebuild index. Wait for the user before searching again.
- GET /agent/v1/list?root=<alias>&path=<relative-folder> lists a folder. Use an empty path to list the root.
- GET /agent/v1/read?root=<alias>&path=<relative-file> returns encoding and content. The encoding is text or base64.
- POST /agent/v1/write accepts {"root":"alias","path":"relative file","content":"file content","encoding":"text"}. Encoding may also be base64. Write files only at my explicit request.
- POST /agent/v1/mkdir accepts {"root":"alias","path":"relative folder"}. Create folders only at my explicit request.

Verify the connector once setup is complete. ${trialInstruction} If a request fails, report the endpoint, HTTP status, and actual error.`;

  return `请配置一个名为「Muse 本机文件」的 Custom Connector。HTTPS 地址为 ${origin}，接口前缀为 /agent/v1。
请用安全卡片收集此连接专用的 agent-token，每次请求发送 Authorization: Bearer <令牌>。使用独立的凭证项，只在请求 ${origin}/agent/v1/ 下的接口时使用。对话中只显示调用结果。

接口返回 JSON。POST 请求发送 Content-Type: application/json。GET 查询参数需做 URL 编码。各路径使用授权别名 root 与该别名内的相对路径 path。
- GET /agent/v1/roots 返回 roots.search、roots.read、roots.write，各自列出对应操作的目录别名。
- GET /agent/v1/locations/desktop 返回 root、path 和 Windows 实际桌面的 absolute_path。需要定位桌面时使用，桌面须在授权范围内。
- POST /agent/v1/search 接收 {"name":"文件名的一部分","limit":30}。可选 root、extensions、modified_since_ms、modified_before_ms 和 cursor。知道盘符时优先传对应的 root。返回 hits、complete、scanning、backend、scopes 及可选 next_cursor、retry_after_ms。命中含 root、path、kind、size、modified_ms。若 scanning 为 true，按 retry_after_ms 等待后再查，先告知当前有哪些结果及未完成的范围。扫描未结束或 complete 为 false 时，标明覆盖还不完整。分页时把 next_cursor 作为下一次的 cursor，其余查询参数保持一致。
- 若搜索返回 HTTP 409 且 error 为 search_index_cleared，告诉用户在桌面连接器的「文件权限」中点「重新建立索引」。等待用户操作后再搜索。
- GET /agent/v1/list?root=<目录别名>&path=<相对目录> 列出目录。列根目录时 path 填空字符串。
- GET /agent/v1/read?root=<目录别名>&path=<相对文件> 返回 encoding 和 content，encoding 为 text 或 base64。
- POST /agent/v1/write 接收 {"root":"目录别名","path":"相对文件","content":"文件内容","encoding":"text"}，encoding 也可以为 base64。写入仅按我的明确请求执行。
- POST /agent/v1/mkdir 接收 {"root":"目录别名","path":"相对目录"}，仅按我的明确请求建目录。

接入后请验证一次。${trialInstruction} 若调用失败，请报告实际接口、HTTP 状态码和错误。`;
}

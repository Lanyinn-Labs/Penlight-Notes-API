#!/usr/bin/env python3
"""Check the official JP Google Play listing and prepare a review-only update candidate."""
import argparse
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = 'com.bushiroad.sirius'
STORE_URL = f'https://play.google.com/store/apps/details?id={PACKAGE}&hl=ja&gl=JP'
VERSION = re.compile(r'\d+\.\d+\.\d+')
MAX_PAGE_BYTES = 4 * 1024 * 1024


def parse_version(page):
    """Require the expected app identity and known version metadata; never guess from page text."""
    identity = False
    structured_version = None
    for match in re.finditer(r'<script\b[^>]*type=[\"\x27]application/ld\+json[\"\x27][^>]*>(.*?)</script>', page, re.S):
        value = json.loads(match[1])
        if not isinstance(value, dict) or value.get('@type') != 'SoftwareApplication':
            continue
        url = urllib.parse.urlsplit(html.unescape(value.get('url', '')))
        if url.hostname == 'play.google.com' and urllib.parse.parse_qs(url.query).get('id') == [PACKAGE]:
            identity = True
            structured_version = value.get('softwareVersion')
    if not identity:
        raise ValueError('Google Play application identity is missing or changed')
    if isinstance(structured_version, str) and VERSION.fullmatch(structured_version):
        return structured_version
    # Verified against the official listing; reject changed schema instead of selecting an unrelated number.
    for match in re.finditer(r"AF_initDataCallback\(\{key:\s*['\"]ds:5['\"][^\n]*?data:\s*", page):
        data, _ = json.JSONDecoder().raw_decode(page[match.end():])
        try:
            value = data[1][2][140][0][0][0]
        except (IndexError, KeyError, TypeError) as error:
            raise ValueError('Google Play version metadata layout changed') from error
        if isinstance(value, str) and VERSION.fullmatch(value):
            return value
    raise ValueError('Google Play does not provide a recognized Android version')


def fetch_version():
    request = urllib.request.Request(STORE_URL, headers={'User-Agent': 'Mozilla/5.0 (Penlight version monitor)'})
    with urllib.request.urlopen(request, timeout=20) as response:
        location = urllib.parse.urlsplit(response.geturl())
        if location.scheme != 'https' or location.hostname != 'play.google.com':
            raise ValueError('Unexpected Google Play redirect')
        if urllib.parse.parse_qs(location.query).get('id') != [PACKAGE]:
            raise ValueError('Unexpected Google Play application')
        data = response.read(MAX_PAGE_BYTES + 1)
    if len(data) > MAX_PAGE_BYTES:
        raise ValueError('Google Play response exceeded the size limit')
    return parse_version(data.decode('utf-8'))


def report(current, latest):
    if not VERSION.fullmatch(current) or not VERSION.fullmatch(latest):
        raise ValueError('Expected a stable three-part version')
    current_tuple = tuple(map(int, current.split('.')))
    latest_tuple = tuple(map(int, latest.split('.')))
    status = 'update_available' if latest_tuple > current_tuple else 'current' if latest_tuple == current_tuple else 'store_older'
    return {'package_id': PACKAGE, 'platform': 'Android', 'region': 'jp',
            'configured_version': current, 'store_version': latest, 'status': status, 'source': STORE_URL}


def candidate_text(value):
    return f'''# 日服安卓适配候选

- 当前配置版本：`{value['configured_version']}`
- Google Play 显示版本：`{value['store_version']}`
- 包名：`{PACKAGE}`
- 来源：[Google Play]({STORE_URL})

此候选仅记录版本变化，尚未修改运行配置或协议。商店显示版本不保证所有设备已完成灰度更新。

## 验证清单

- [ ] 取得并核对 Android 新构建的版本和包名
- [ ] 验证现有账号的 Version、Whoami 和 GetPlayerData
- [ ] 比较客户端版本要求、认证和 Protobuf 变化
- [ ] 比较 Master 密钥、IV 和本地存档格式；变化时重新适配工具
- [ ] 必要时更新 Sirius 来源提交、许可证记录和文件摘要
- [ ] 调整配置后验证 Master、公告、玩家资料和排行
- [ ] 通过 CI 及在线验证后，更新版本记录并发布
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, default=ROOT / 'config/jp.example.json')
    parser.add_argument('--html-file', type=Path, help='Inspect a saved official listing for local verification')
    parser.add_argument('--write-candidate', type=Path)
    parser.add_argument('--github-output', type=Path)
    args = parser.parse_args()
    config = json.loads(args.config.read_text(encoding='utf-8'))
    if config.get('region') != 'jp' or config.get('platform', '').lower() != 'android':
        raise ValueError('Version monitor requires a JP Android configuration')
    latest = parse_version(args.html_file.read_text(encoding='utf-8')) if args.html_file else fetch_version()
    value = report(config['client_version'], latest)
    if args.write_candidate:
        if value['status'] == 'update_available':
            args.write_candidate.parent.mkdir(parents=True, exist_ok=True)
            args.write_candidate.write_text(candidate_text(value), encoding='utf-8', newline='\n')
        elif value['status'] == 'current' and args.write_candidate.exists():
            args.write_candidate.unlink()
    if args.github_output:
        with args.github_output.open('a', encoding='utf-8', newline='\n') as output:
            output.write(f"update_available={'true' if value['status'] == 'update_available' else 'false'}\n")
            output.write(f"store_version={latest}\n")
    value['checked_at'] = datetime.now(timezone.utc).isoformat()
    print(json.dumps(value, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, UnicodeError, OSError, urllib.error.URLError) as error:
        raise SystemExit(f'Android version check failed: {error}')

#!/usr/bin/env python3
"""Compare the official JP Google Play version with the configured client version."""
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


def read_version(path):
    value = json.loads(path.read_text(encoding='utf-8'))['client_version']
    if not isinstance(value, str) or not VERSION.fullmatch(value):
        raise ValueError('Expected a stable three-part version in the client bundle')
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--client-config', type=Path, default=ROOT / 'data/jp-client.json')
    parser.add_argument('--html-file', type=Path, help='Inspect a saved official listing for local verification')
    args = parser.parse_args()
    current = read_version(args.client_config)
    latest = parse_version(args.html_file.read_text(encoding='utf-8')) if args.html_file else fetch_version()
    value = report(current, latest)
    value['checked_at'] = datetime.now(timezone.utc).isoformat()
    print(json.dumps(value, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, UnicodeError, OSError, urllib.error.URLError) as error:
        raise SystemExit(f'Android version check failed: {error}')

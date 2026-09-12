import json, os, pathlib, re, subprocess, sys, tempfile, time, urllib.error, urllib.request

binary = pathlib.Path(sys.argv[1]).resolve()
out_path = pathlib.Path(sys.argv[2]).resolve()
records = []
with tempfile.TemporaryDirectory(prefix='julie_plan2_live_') as tmp:
    root = pathlib.Path(tmp)
    workspace = root / 'project'
    workspace.mkdir()
    (workspace / '.git').mkdir()
    value = 'pub const PAGED_VALUE: &str = concat!(\r\n    "café",\r\n    "雪",\r\n);'
    long_value = 'pub const LONG_VALUE: &str = concat!(\r\n' + ''.join(
        '    "' + 'x' * 150 + '",\r\n' for _ in range(400)
    ) + ');'
    body_source = value + '\r\n'
    reference_source = 'pub fn target() {}\r\npub fn caller() { target(); target(); target(); }\r\n'
    (workspace / 'bodies.rs').write_bytes(body_source.encode())
    (workspace / 'long.rs').write_bytes((long_value + '\r\n').encode())
    (workspace / 'references.rs').write_bytes(reference_source.encode())
    home = root / 'service-home'
    env = dict(os.environ, JULIE_HOME=str(home), JULIE_EMBEDDING_PROVIDER='none')
    log = open(root / 'service.log', 'wb')
    service = subprocess.Popen([str(binary), 'service'], env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        discovery = home / 'service.json'
        deadline = time.monotonic() + 30
        while not discovery.exists():
            if service.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('isolated service failed to start')
            time.sleep(.1)
        info = json.loads(discovery.read_text())
        headers = {'Authorization': 'Bearer ' + info['token'], 'Content-Type': 'application/json'}
        def api(name, arguments):
            request = urllib.request.Request(f"http://127.0.0.1:{info['port']}/api/{name}", data=json.dumps(arguments).encode(), headers=headers)
            with urllib.request.urlopen(request, timeout=60) as response:
                reply = json.load(response)
            return reply['result']
        def text(result):
            return '\n'.join(x.get('text', '') for x in result.get('content', []) if x.get('type') == 'text')
        def pages(result):
            structured = result.get('structuredContent', result.get('structured_content'))
            assert structured is not None, result
            return structured['body_pages']
        def next_args(line, name):
            prefix = f'next: {name} '
            assert line.startswith(prefix), line
            return json.loads(line[len(prefix):])
        indexed = api('manage_workspace', {'operation': 'index', 'path': str(workspace), 'force': True, 'semantics': 'off'})
        args = {'symbol': 'target', 'limit': 1, 'include_definition': False, 'reference_kind': 'call', 'workspace': str(workspace), 'semantics': 'off'}
        references = []
        for _ in range(5):
            result = api('fast_refs', args)
            references.extend(result['structuredContent']['references'])
            follow = [line for line in text(result).splitlines() if line.startswith('next: fast_refs ')]
            if not follow:
                break
            args = next_args(follow[0], 'fast_refs')
            assert args['workspace'] and args['semantics'] == 'off' and args['include_definition'] is False
        assert len(references) == 3 and len({row['id'] for row in references}) == 3, (indexed, references, result)
        assert all(row['reference_site_is_exact'] for row in references)
        records.append('three same-line references preserved across independent pages')
        continuations = {}
        for name in ('get_symbols', 'deep_dive'):
            args = {'workspace': str(workspace), 'body_limit': 1, 'semantics': 'off'}
            args.update({'file_path': 'bodies.rs', 'target': 'PAGED_VALUE', 'mode': 'full'} if name == 'get_symbols' else {'symbol': 'PAGED_VALUE', 'context_file': 'bodies.rs', 'depth': 'full'})
            chunks = []
            for index in range(8):
                result = api(name, args)
                page = pages(result)[0]
                chunks.append(page['text'])
                display = text(result)
                assert 'page_text=' in display and page['text'].rstrip('\r\n') in display, (name, index, result)
                assert page['complete'] is False
                follow = page['continuation']
                if not follow:
                    assert page['end_reached']
                    break
                continuations.setdefault(name, next_args(follow, name))
                args = next_args(follow, name)
                assert args['source_hash'] == page['source_hash'] and args['workspace']
            assert ''.join(chunks) == value, (name, chunks)
            records.append(name + ' exact UTF8/CRLF body pages reassembled')
        meta = {'io.modelcontextprotocol/protocolVersion': '2026-07-28', 'io.modelcontextprotocol/clientCapabilities': {}, 'io.modelcontextprotocol/clientInfo': {'name': 'plan2-live-probe', 'version': '1'}}
        mcp_args = {'workspace': str(workspace), 'file_path': 'bodies.rs', 'target': 'PAGED_VALUE', 'mode': 'full', 'body_limit': 1, 'semantics': 'off'}
        rpc = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'_meta': meta, 'name': 'get_symbols', 'arguments': mcp_args}}
        mcp_headers = dict(headers, Accept='application/json, text/event-stream', **{'Mcp-Method': 'tools/call', 'Mcp-Name': 'get_symbols', 'MCP-Protocol-Version': '2026-07-28'})
        request = urllib.request.Request(f"http://127.0.0.1:{info['port']}/mcp", data=json.dumps(rpc).encode(), headers=mcp_headers)
        with urllib.request.urlopen(request, timeout=60) as response:
            mcp = json.load(response)['result']
        assert pages(mcp)[0]['text'] == value.splitlines(keepends=True)[0]
        records.append('Streamable HTTP MCP returns the same exact body page')
        for command in (
            ['symbols', 'bodies.rs', '--mode', 'full', '--target', 'PAGED_VALUE'],
            ['deep-dive', 'PAGED_VALUE', '--depth', 'full', '--context-file', 'bodies.rs'],
        ):
            args = [str(binary), '--workspace', str(workspace), '--semantics', 'off', *command, '--body-limit', '1', '--json']
            process = subprocess.run(args, env=env, cwd=workspace, capture_output=True, text=True, timeout=60)
            assert process.returncode == 0, process.stderr
            reply = json.loads(process.stdout)
            assert pages(reply['reply']['result'])[0]['text'] == value.splitlines(keepends=True)[0]
        records.append('both named CLI wrappers return matching body pages')
        for name in ('get_symbols', 'deep_dive'):
            args = {'workspace': str(workspace), 'semantics': 'off'}
            args.update({'file_path': 'long.rs', 'target': 'LONG_VALUE', 'mode': 'full'} if name == 'get_symbols' else {'symbol': 'LONG_VALUE', 'context_file': 'long.rs', 'depth': 'full'})
            result = api(name, args)
            page = pages(result)[0]
            assert page['returned_range'] == [0, 100] and page['complete'] is False and page['continuation']
            args['body_limit'] = 500
            result = api(name, args)
            page = pages(result)[0]
            assert page['text'] == long_value and page['complete'] is True
            assert long_value.replace('\r\n', '\n') in text(result).replace('\r\n', '\n')
        records.append('both tools bound default bodies and expose complete60k multiline bodies explicitly')
        (workspace / 'bodies.rs').write_bytes(body_source.replace('PAGED_VALUE', 'RENAMED_VALUE').encode())
        api('manage_workspace', {'operation': 'index', 'path': str(workspace), 'force': True, 'semantics': 'off'})
        for name, args in continuations.items():
            try:
                result = api(name, args)
            except urllib.error.HTTPError as error:
                message = error.read().decode()
                assert 'source changed' in message and 'body_offset=0' in message, message
            else:
                assert result.get('isError') and 'source changed' in text(result), (name, result)
        records.append('both stale continuations refused after source rename/reindex')
        out_path.write_text(json.dumps({'checks': records, 'count': len(records)}, indent=2) + '\n')
        print(out_path.read_text())
    finally:
        subprocess.run([str(binary), 'service', 'stop'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        try:
            service.wait(timeout=10)
        except subprocess.TimeoutExpired:
            service.terminate()
            service.wait(timeout=10)
        log.close()

"""Fail-closed JSON equivalence proof for freshly allocated authoring IDs.

Every field and array position is retained. Only explicit persisted entity IDs
and schema-known references may be transformed by one global bijection. Existing
seed IDs are protected. Component bindings are rejected because their embedded
historical snapshots require a separate identity model.
"""
import copy
import hashlib
import json


def require(condition, message):
    if not condition:
        raise ValueError(message)


def strict_load(data):
    def object_pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    def invalid(value):
        raise ValueError('nonfinite JSON value')
    return json.loads(data, object_pairs_hook=object_pairs, parse_constant=invalid)


def canonical(value):
    # Preserves every value/type (including float versus int and signed zero),
    # list order, string and opaque encoded image; only object-key order changes.
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def pointer(path):
    return '/'.join(str(part).replace('~', '~0').replace('/', '~1') for part in path)


def entities(document):
    doc = document['doc']
    result = {}
    def add(path, kind, value, owner=None):
        require(type(value['id']) is int and 0 < value['id'] < 2**64, 'invalid entity ID')
        result[path + ('id',)] = {'kind': kind, 'id': value['id'], 'owner': owner, 'value': value}
    for index, layer in enumerate(doc['layers']):
        path = ('doc', 'layers', index)
        add(path, 'layer', layer)
        for shape_index, shape in enumerate(layer['kind'].get('Vector', {}).get('shapes', [])):
            add(path + ('kind', 'Vector', 'shapes', shape_index), 'shape', shape, index)
    for field, kind in [('artboards', 'artboard'), ('layout_tokens', 'token'), ('comments', 'comment')]:
        for index, value in enumerate(doc.get(field, [])):
            add(('doc', field, index), kind, value)
    ids = [node['id'] for node in result.values()]
    require(len(ids) == len(set(ids)), 'duplicate entity IDs')
    return result


def references(document, nodes):
    by_id = {node['id']: node for node in nodes.values()}
    result = {}
    def add(path, target, kind, owner=None, frame=False, group=False):
        if target is None:
            return
        require(type(target) is int, 'noninteger reference')
        node = by_id.get(target)
        require(node is not None and node['kind'] == kind, 'unresolved or wrong-kind reference: ' + pointer(path))
        require(owner is None or node['owner'] == owner, 'layout parent crosses vector layers')
        require(not frame or node['value'].get('layout', {}).get('frame') is True, 'reference requires a frame')
        require(not group or node['value'].get('is_group') is True, 'reference requires a layer group')
        result[path] = {'target': target, 'kind': kind}
    for path, node in nodes.items():
        base, value = path[:-1], node['value']
        if node['kind'] == 'layer':
            add(base + ('parent',), value.get('parent'), 'layer', group=True)
        elif node['kind'] == 'shape':
            layout = value.get('layout', {})
            require(layout.get('component') is None, 'component identity snapshots are outside this proof')
            add(base + ('layout', 'parent'), layout.get('parent'), 'shape', owner=node['owner'], frame=True)
            for key, token in layout.get('tokens', {}).items():
                require(key in {'fill', 'stroke', 'gap', 'padding', 'radius'}, 'unknown token reference')
                add(base + ('layout', 'tokens', key), token, 'token')
            for index, interaction in enumerate(layout.get('interactions', [])):
                action = interaction['action']
                action_path = base + ('layout', 'interactions', index, 'action')
                if isinstance(action, str):
                    require(action in {'Back', 'CloseOverlay'}, 'unknown prototype action')
                else:
                    require(len(action) == 1, 'unknown prototype action')
                    key, target = next(iter(action.items()))
                    if key in {'Navigate', 'OpenOverlay'}:
                        add(action_path + (key, 'target'), target['target'], 'shape', frame=True)
                    elif key == 'SetVariant':
                        add(action_path + (key, 'component'), target['component'], 'shape')
                    else:
                        raise ValueError('unknown prototype action')
            text = value['geom'].get('Text')
            if text is not None:
                on_path = text.get('on_path')
                if on_path is not None:
                    add(base + ('geom', 'Text', 'on_path', 'path_id'), on_path['path_id'], 'shape')
                thread = text.get('thread')
                if thread is not None:
                    for key in ('story', 'prev', 'next'):
                        add(base + ('geom', 'Text', 'thread', key), thread.get(key), 'shape')
        elif node['kind'] == 'comment':
            add(base + ('frame_id',), value.get('frame_id'), 'shape', frame=True)
    for index, track in enumerate(document['doc'].get('motion', {}).get('tracks', [])):
        add(('doc', 'motion', 'tracks', index, 'shape'), track['shape'], 'shape')
    # Layer/layout parent graphs must be acyclic, not just resolvable.
    for node in nodes.values():
        if node['kind'] not in {'layer', 'shape'}:
            continue
        seen = {node['id']}
        current = node
        while True:
            parent = (current['value'].get('parent') if current['kind'] == 'layer'
                      else current['value'].get('layout', {}).get('parent'))
            if parent is None:
                break
            require(parent not in seen, 'cyclic parent graph')
            seen.add(parent)
            current = by_id[parent]
    return result


def prove(reference_bytes, candidate_bytes, seed_bytes):
    try:
        left, right, seed = map(strict_load, (reference_bytes, candidate_bytes, seed_bytes))
        old, new, original = map(entities, (left, right, seed))
        require(old.keys() == new.keys(), 'entity positions or kinds changed')
        protected = {node['id'] for node in original.values()}
        mapping = {}
        changes = []
        for path, node in old.items():
            other = new[path]
            require(node['kind'] == other['kind'], 'entity kind changed')
            mapping[node['id']] = other['id']
            if node['id'] != other['id']:
                require(node['kind'] in {'layer', 'shape'}, 'only fresh layer/shape IDs may differ')
                require(node['id'] not in protected and other['id'] not in protected, 'seed ID changed or reused')
                changes.append({'entity_path': pointer(path), 'kind': node['kind'],
                                'baseline_id': node['id'], 'candidate_id': other['id']})
        require(len(mapping) == len(set(mapping.values())), 'ID map is not bijective')
        old_refs, new_refs = references(left, old), references(right, new)
        require(old_refs.keys() == new_refs.keys(), 'reference graph structure changed')
        reference_changes = []
        for path, edge in old_refs.items():
            other = new_refs[path]
            require(edge['kind'] == other['kind'] and mapping[edge['target']] == other['target'],
                    'reference did not follow the same ID bijection: ' + pointer(path))
            if edge['target'] != other['target']:
                reference_changes.append({'reference_path': pointer(path),
                                          'baseline_id': edge['target'], 'candidate_id': other['target']})
        # A changed ID may not survive in an unclassified integer slot. This
        # also fails closed for a future reference field not yet modeled here;
        # it never heuristically rewrites unrelated numeric values.
        permitted = set(old) | set(old_refs)
        changed_ids = {old_id for old_id, new_id in mapping.items() if old_id != new_id}
        def check_occurrences(value, path=()):
            if isinstance(value, dict):
                for key, child in value.items():
                    require(not (key.isdigit() and int(key) in changed_ids),
                            'changed ID occurs in an unsupported keyed map')
                    check_occurrences(child, path + (key,))
            elif isinstance(value, list):
                for index, child in enumerate(value):
                    check_occurrences(child, path + (index,))
            elif type(value) is int and value in changed_ids:
                require(path in permitted, 'changed ID in unclassified integer field: ' + pointer(path))
        check_occurrences(left)
        normalized = copy.deepcopy(left)
        for path in list(old) + list(old_refs):
            parent = normalized
            for part in path[:-1]:
                parent = parent[part]
            parent[path[-1]] = mapping[parent[path[-1]]]
        normalized_bytes, right_bytes = canonical(normalized), canonical(right)
        require(normalized_bytes == right_bytes, 'a non-ID value, type, field, or array order differs')
        return {'equal_after_consistent_id_remap': True, 'bijection': True,
                'all_non_id_fields_and_array_order_equal': True,
                'seed_entity_ids_protected': len(protected), 'validated_entities': len(old),
                'validated_reference_edges': len(old_refs), 'unclassified_changed_id_occurrences': 0,
                'changed_entity_ids': changes,
                'changed_reference_edges': reference_changes,
                'remapped_baseline_canonical_sha256': hashlib.sha256(normalized_bytes).hexdigest(),
                'candidate_canonical_sha256': hashlib.sha256(right_bytes).hexdigest()}
    except (ValueError, KeyError, TypeError) as error:
        return {'equal_after_consistent_id_remap': False, 'reason': str(error)}

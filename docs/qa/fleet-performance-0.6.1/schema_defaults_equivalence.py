"""Additive current-schema evidence, separate from raw byte/pixel equality."""
from collections import Counter
import copy
import hashlib
import subprocess

from document_id_equivalence import canonical, entities, prove, require, strict_load


def verify_source(repository, revision):
    source = subprocess.check_output(['git', 'show', revision + ':src/document.rs'], cwd=repository)
    require(source.count(b'#[serde(default = "one")]\n    pub fill_opacity: f32,') == 2,
            'Shape/Layer fill opacity serde declarations changed')
    require(source.count(b'#[serde(default)]\n    pub blend_interior: bool,') == 2,
            'Shape/Layer blend interior serde declarations changed')
    require(b'fn one() -> f32 {\n    1.0\n}' in source
            and b'#[serde(default)]\n    pub text_wrap_above_only: bool,' in source,
            'current document defaults changed')
    return {'source_commit': revision, 'document_rs_sha256': hashlib.sha256(source).hexdigest(),
            'allowed_defaults': {'Shape.fill_opacity': 1.0, 'Layer.fill_opacity': 1.0,
                                 'Shape.blend_interior': False, 'Layer.blend_interior': False,
                                 'Document.text_wrap_above_only': False}}


def prove_with_current_defaults(reference_bytes, candidate_bytes, seed_bytes):
    """Initialize absent reference fields only, then apply the strict ID proof.

    No candidate values, unknown fields, list ordering or opaque image bytes
    are removed. A successful result never changes raw equality findings.
    """
    try:
        reference, candidate = map(strict_load, (reference_bytes, candidate_bytes))
        initialized = copy.deepcopy(reference)
        counts = Counter()
        def add(value, other, key, default):
            if key in value:
                return
            require(key in other and type(other[key]) is type(default) and other[key] == default,
                    'new field is not the exact current default')
            value[key] = default
            counts[key] += 1
        add(initialized['doc'], candidate['doc'], 'text_wrap_above_only', False)
        original, current = entities(initialized), entities(candidate)
        require(original.keys() == current.keys(), 'entity positions changed')
        for path, node in original.items():
            if node['kind'] in ('layer', 'shape'):
                add(node['value'], current[path]['value'], 'fill_opacity', 1.0)
                add(node['value'], current[path]['value'], 'blend_interior', False)
        proof = prove(canonical(initialized), candidate_bytes, seed_bytes)
        return {'common_authored_state_equal_after_current_defaults_and_id_remap':
                    proof['equal_after_consistent_id_remap'],
                'exact_default_fields_added_to_reference_only': dict(counts),
                'strict_id_and_reference_proof': proof,
                'raw_equality_unchanged': True,
                'scope': 'Only absent reference-side current serde defaults are initialized. Raw document bytes and image pixels remain separately compared; no unknown field or candidate value is discarded.'}
    except (ValueError, KeyError, TypeError) as error:
        return {'common_authored_state_equal_after_current_defaults_and_id_remap': False,
                'raw_equality_unchanged': True, 'reason': str(error)}

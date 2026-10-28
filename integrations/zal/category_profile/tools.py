"""Fixed data-only capability schemas; discovery does not require a worker/GHC."""
from copy import deepcopy

OPERATIONS = ['check-category', 'compose-path', 'invertible', 'check-functor', 'compose-functors', 'compose-symbolic']


def toolkit():
    obj = {'type': 'object'}
    text = {'type': 'string'}
    shared = {'library': obj, 'entry_digest': text, 'application': obj, 'record': obj}
    definitions = [
        ('prepare', 'Retain an exact entry/context/request and authored or exact-declaration interpretation; no finite law is certified by preparation.',
         ['library', 'entry_digest', 'operation', 'parameters', 'interpretation'],
         {'library': obj, 'entry_digest': text, 'operation': {'enum': OPERATIONS}, 'parameters': obj,
          'interpretation': obj, 'budget': {'type': 'integer', 'minimum': 0, 'maximum': 1000000}}),
        ('check', 'Run the supplied finite request only with explicit trusted operator approval; otherwise Unavailable. Refutations cover the whole request.',
         ['library', 'entry_digest', 'application'], {k: shared[k] for k in ['library', 'entry_digest', 'application']}),
        ('verify', 'Recompute an entry-bound record; a matching negative or incomplete record is not a positive mathematical claim.',
         ['library', 'entry_digest', 'application', 'record'], shared),
        ('audit', 'Fresh static table audit of operator-pinned prior evidence; launches no worker and does not authenticate an author.',
         ['library', 'entry_digest', 'application', 'record'], shared)]
    return {'schema': 'zal/library-category-toolkit/1', 'installed_service': False,
            'transport': 'Pure sharing.dispatch_tool or category_profile standalone CLI; no deployed service implied.',
            'runtime_default': 'Unavailable until a trusted operator configures an approved runtime.',
            'tools': [{'name': 'zal_library_category_' + name, 'description': description,
                       'input_schema': {'type': 'object', 'required': required,
                                        'properties': deepcopy(properties), 'additionalProperties': False}}
                      for name, description, required, properties in definitions]}

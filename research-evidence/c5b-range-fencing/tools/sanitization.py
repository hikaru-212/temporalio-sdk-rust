"""Pure publication projection. Does not execute tests or read environment configuration."""
import base64
import hashlib
import re
import uuid

VERSION = 'c5b-public-projection-v1'

def digest(data):
    return hashlib.sha256(data).hexdigest()

class Projection:
    def __init__(self, arm):
        self.arm = arm
        self.maps = {}
        self.actions = []
        self.forbidden_values = set()

    def alias(self, kind, value, path):
        if value is None or value == '':
            return value
        key = (kind, value)
        if key not in self.maps:
            index = 1 + sum(k[0] == kind for k in self.maps)
            self.maps[key] = f'<{self.arm}:{kind}:{index}>'
        public = self.maps[key]
        self.actions.append({'path': path, 'rule': kind + '-stable-alias', 'original_value_sha256_utf8': digest(value.encode()), 'public_value': public})
        self.forbidden_values.add(value)
        return public

    def withheld(self, value, path, reason):
        import json
        raw = value.encode() if isinstance(value,str) else json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
        if isinstance(value,str) and len(value)>20:
            self.forbidden_values.add(value)
        marker = {'publication_status':'withheld', 'reason':reason, 'original_value_sha256':digest(raw), 'hash_encoding':'UTF-8 string' if isinstance(value,str) else 'canonical JSON UTF-8'}
        self.actions.append({'path':path,'rule':reason,'original_value_sha256':digest(raw)})
        return marker

    def text(self, value, path):
        original = value
        # Function identities remain available; raw pointer values are not durable transaction IDs.
        for token in re.findall(r'\*sqlite\.(?:db|conn):0x[0-9a-f]+', value):
            value = value.replace(token,self.alias('transaction' if '.db:' in token else 'connection',token,path))
        # Keep repository-relative stack source locations, independent of the local checkout name.
        value = re.sub(r'/(?:[^\s:\"\x27]+/)*temporal-ccpo-c5b2-p2-20261026/', '<SERVER_CHECKOUT>/', value)
        value = re.sub(r'/(?:[^\s:\"\x27]+/)*ccpo-c5b2-(?:main|positive|negative)-20261026/run-01/runtime/[^\s\"\x27?]+', '<SQLITE_DB_'+self.arm.upper()+'>',value)
        value = re.sub(r'/(?:home|tmp|var/tmp|usr/local|root)/[^\s\"\x27`<>?,)]+','<LOCAL_PATH>',value)
        value = re.sub(r'\b(?:127\.0\.0\.1|0\.0\.0\.0|10\.[0-9]+\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+)(?::[0-9]+)?\b','<LOCAL_ENDPOINT>',value)
        if value != original:
            self.actions.append({'path':path,'rule':'text-path-endpoint-normalization','original_value_sha256_utf8':digest(original.encode())})
        return value

    def stack(self, value, path):
        frames=[]
        lines=value.splitlines()
        for i,line in enumerate(lines):
            if not line.startswith('\t') or '.go:' not in line:
                continue
            location=self.text(line.strip().split(' +0x',1)[0],path)
            fun=lines[i-1] if i else ''
            # Omit argument addresses but retain Go receiver and function names.
            fun=re.sub(r'\((?:0x|\{|\[|\.\.\.|\)|[0-9]).*$', '(arguments omitted)',fun)
            fun=re.sub(r'0x[0-9a-f]+','<ADDRESS>',fun)
            fun=re.sub(r'in goroutine [0-9]+', 'in goroutine <ID>', fun)
            frames.append({'function':fun,'source':location})
        self.actions.append({'path':path,'rule':'stack-to-symbol-source-frames; goroutine numbers/argument addresses omitted','original_value_sha256_utf8':digest(value.encode())})
        return frames

    def value(self, value, path='value', key=''):
        normalized=key.lower().replace('_','')
        if normalized in ['tasktoken','tokenhex']:
            return self.withheld(value,path,'packed-task-token-withheld')
        if normalized in ['branchtoken','nexttoken','requesttoken'] and isinstance(value,str):
            return self.alias('branch' if normalized=='branchtoken' else 'page-token',value,path)
        if normalized=='stack' and isinstance(value,str):
            return self.stack(value,path)
        if isinstance(value,dict):
            return {k:self.value(v,path+'/'+k,k) for k,v in value.items()}
        if isinstance(value,list):
            return [self.value(v,path+'/'+str(i),key) for i,v in enumerate(value)]
        if isinstance(value,str):
            if key in ['Data','ExecutionInfoBlob','ExecutionStateBlob','CassandraBlob'] and len(value)>20:
                return self.withheld(value,path,'opaque-serialized-data-withheld')
            if key in ['NamespaceID','RunID','TreeID','BranchID'] and len(value)==24:
                try:
                    raw=base64.b64decode(value,validate=True)
                    if len(raw)==16:
                        result=str(uuid.UUID(bytes=raw));self.actions.append({'path':path,'rule':'base64-16-byte-synthetic-UUID-to-text','original_value_sha256_utf8':digest(value.encode()),'public_value':result});return result
                except (ValueError,TypeError):pass
            return self.text(value,path)
        return value

    def event(self, record):
        import json
        result={k:record[k] for k in ['sequence','stage','utc','raw_stdout_line']}
        v=record['value'];p='seq:'+str(record['sequence'])+'/value'
        if record['stage']=='sql-config':
            keep=['PluginName','ConnectAttributes','MaxConns','MaxIdleConns','MaxConnLifetime','TaskScanPartitions']
            v={k:v[k] for k in keep if k in v}
            self.actions.append({'path':p,'rule':'sql-config allowlist; User Password PasswordCommand DatabaseName endpoints TLS omitted'})
        elif record['stage']=='internal-request':
            v={'original_normalized_request':self.withheld(v,p,'serialized-internal-request-withheld'), **{k:v[k] for k in ['ArchetypeID','Mode','NewWorkflowSnapshot','RangeID','ShardID']}, 'history_batch_count':len(v['UpdateWorkflowNewEvents']), 'new_workflow_event_count':len(v['NewWorkflowNewEvents'] or []), 'semantic_request_checks':'normal-request-shape-verified and ownership-audit events'}
        result['value']=self.value(v,p)
        result['original_record_value_sha256_canonical_json']=digest(json.dumps(record['value'],sort_keys=True,separators=(',',':'),ensure_ascii=False).encode())
        return result

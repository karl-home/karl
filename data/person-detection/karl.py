import os
import grpc

import request_pb2
import request_pb2_grpc

class KarlSDK:
    def __init__(self):
        self.global_hook_id = os.environ.get('GLOBAL_HOOK_ID')
        self.hook_id = os.environ.get('HOOK_ID')
        self.token = os.environ.get('PROCESS_TOKEN')
        self.params = {}
        self.returns = {}
        for x in os.environ.get('KARL_PARAMS').split(':'):
            y = x.split(';')
            self.params[y[0]] = y[1]
        for x in os.environ.get('KARL_RETURNS').split(':'):
            y = x.split(';')
            self.returns[y[0]] = y[1].split(',')
        self.channel = grpc.insecure_channel('localhost:59583')
        self.stub = request_pb2_grpc.KarlHostStub(self.channel)

    def get_triggered(self):
        tag = os.environ.get('TRIGGERED_TAG')
        timestamp = os.environ.get('TRIGGERED_TIMESTAMP')
        return self._get_tag(tag, timestamp, timestamp).data[0]

    def get(self, param, lower_timestamp, upper_timestamp):
        if param in self.params:
            tag = self.params[param]
            return self._get_tag(tag, lower_timestamp, upper_timestamp)

    def _get_tag(self, tag, lower_timestamp, upper_timestamp):
        req = request_pb2.GetData(process_token=self.token,
                                  tag=tag,
                                  lower=lower_timestamp,
                                  upper=upper_timestamp)
        res = self.stub.Get(req)
        if res is None:
            print('no result')
        return res

    def push(self, return_name, data):
        if return_name in self.tags:
            for tag in self.tags[return_name]:
                tag = '{}.{}'.format(self.hook_id, tag)
                req = request_pb2.PushData(process_token=self.token,
                                           tag=tag,
                                           data=data)
                self.stub.Push(req)

    def network_access(stub, domain):
        req = request_pb2.NetworkAccess(process_token=self.token,
                                        domain=domain,
                                        method="GET")
        res = stub.Network(req)
        if res is None:
            print('no result')
            return None
        print(res.status_code)
        print(type(res.data))
        print(len(res.data))
        return res

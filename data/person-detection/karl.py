import os
import grpc

import request_pb2
import request_pb2_grpc

class KarlAPI:
    def __init__(self):
        self.global_hook_id = os.environ.get('GLOBAL_HOOK_ID')
        self.hook_id = os.environ.get('HOOK_ID')
        self.token = os.environ.get('PROCESS_TOKEN')
        self.channel = grpc.insecure_channel('localhost:59583')
        self.stub = request_pb2_grpc.KarlHostStub(self.channel)

    def get(self, tag, lower_timestamp, upper_timestamp):
        req = request_pb2.GetData(process_token=self.token,
                                  tag=tag,
                                  lower=lower_timestamp,
                                  upper=upper_timestamp)
        res = self.stub.Get(req)
        if res is None:
            print('no result')
            return None
        print(type(res.data))
        print(len(res.data))
        return res.data

    def push(self, tag, data):
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

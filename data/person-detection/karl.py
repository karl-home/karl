import os
import grpc

import request_pb2
import request_pb2_grpc

class KarlSDK:
    def __init__(self):
        self.global_module_id = os.environ.get('GLOBAL_MODULE_ID')
        self.module_id = os.environ.get('MODULE_ID')
        self.token = os.environ.get('PROCESS_TOKEN')
        self.channel = grpc.insecure_channel('localhost:59583')
        self.stub = request_pb2_grpc.KarlHostStub(self.channel)

    def get_event(self, input_):
        tag = "{}.{}".format(self.module_id, input_)
        req = request_pb2.GetEventData(process_token=self.token,
                                       tag=tag)
        res = self.stub.GetEvent(req)
        if res is None:
            print('no result')
        return res.data[0]

    def get(self, input_, lower_timestamp, upper_timestamp):
        tag = "{}.{}".format(self.module_id, input_)
        req = request_pb2.GetData(process_token=self.token,
                                  tag=tag,
                                  lower=lower_timestamp,
                                  upper=upper_timestamp)
        res = self.stub.Get(req)
        if res is None:
            print('no result')
        return res

    def push(self, output, data):
        tag = "{}.{}".format(self.module_id, output)
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

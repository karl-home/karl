# Generated by the gRPC Python protocol compiler plugin. DO NOT EDIT!
"""Client and server classes corresponding to protobuf-defined services."""
import grpc

from google.protobuf import empty_pb2 as google_dot_protobuf_dot_empty__pb2
from . import request_pb2 as request__pb2


class KarlHostStub(object):
    """Missing associated documentation comment in .proto file."""

    def __init__(self, channel):
        """Constructor.

        Args:
            channel: A grpc.Channel.
        """
        self.StartCompute = channel.unary_unary(
                '/request.KarlHost/StartCompute',
                request_serializer=request__pb2.ComputeRequest.SerializeToString,
                response_deserializer=request__pb2.NotifyStart.FromString,
                )
        self.Network = channel.unary_unary(
                '/request.KarlHost/Network',
                request_serializer=request__pb2.NetworkAccess.SerializeToString,
                response_deserializer=request__pb2.NetworkAccessResult.FromString,
                )
        self.Get = channel.unary_unary(
                '/request.KarlHost/Get',
                request_serializer=request__pb2.GetData.SerializeToString,
                response_deserializer=request__pb2.GetDataResult.FromString,
                )
        self.Push = channel.unary_unary(
                '/request.KarlHost/Push',
                request_serializer=request__pb2.PushData.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )


class KarlHostServicer(object):
    """Missing associated documentation comment in .proto file."""

    def StartCompute(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def Network(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def Get(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def Push(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')


def add_KarlHostServicer_to_server(servicer, server):
    rpc_method_handlers = {
            'StartCompute': grpc.unary_unary_rpc_method_handler(
                    servicer.StartCompute,
                    request_deserializer=request__pb2.ComputeRequest.FromString,
                    response_serializer=request__pb2.NotifyStart.SerializeToString,
            ),
            'Network': grpc.unary_unary_rpc_method_handler(
                    servicer.Network,
                    request_deserializer=request__pb2.NetworkAccess.FromString,
                    response_serializer=request__pb2.NetworkAccessResult.SerializeToString,
            ),
            'Get': grpc.unary_unary_rpc_method_handler(
                    servicer.Get,
                    request_deserializer=request__pb2.GetData.FromString,
                    response_serializer=request__pb2.GetDataResult.SerializeToString,
            ),
            'Push': grpc.unary_unary_rpc_method_handler(
                    servicer.Push,
                    request_deserializer=request__pb2.PushData.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
    }
    generic_handler = grpc.method_handlers_generic_handler(
            'request.KarlHost', rpc_method_handlers)
    server.add_generic_rpc_handlers((generic_handler,))


 # This class is part of an EXPERIMENTAL API.
class KarlHost(object):
    """Missing associated documentation comment in .proto file."""

    @staticmethod
    def StartCompute(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlHost/StartCompute',
            request__pb2.ComputeRequest.SerializeToString,
            request__pb2.NotifyStart.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def Network(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlHost/Network',
            request__pb2.NetworkAccess.SerializeToString,
            request__pb2.NetworkAccessResult.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def Get(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlHost/Get',
            request__pb2.GetData.SerializeToString,
            request__pb2.GetDataResult.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def Push(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlHost/Push',
            request__pb2.PushData.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)


class KarlControllerStub(object):
    """Missing associated documentation comment in .proto file."""

    def __init__(self, channel):
        """Constructor.

        Args:
            channel: A grpc.Channel.
        """
        self.HostRegister = channel.unary_unary(
                '/request.KarlController/HostRegister',
                request_serializer=request__pb2.HostRegisterRequest.SerializeToString,
                response_deserializer=request__pb2.HostRegisterResult.FromString,
                )
        self.ForwardNetwork = channel.unary_unary(
                '/request.KarlController/ForwardNetwork',
                request_serializer=request__pb2.NetworkAccess.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.ForwardGet = channel.unary_unary(
                '/request.KarlController/ForwardGet',
                request_serializer=request__pb2.GetData.SerializeToString,
                response_deserializer=request__pb2.GetDataResult.FromString,
                )
        self.ForwardPush = channel.unary_unary(
                '/request.KarlController/ForwardPush',
                request_serializer=request__pb2.PushData.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.ForwardState = channel.unary_unary(
                '/request.KarlController/ForwardState',
                request_serializer=request__pb2.StateChange.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.FinishCompute = channel.unary_unary(
                '/request.KarlController/FinishCompute',
                request_serializer=request__pb2.NotifyEnd.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.Heartbeat = channel.unary_unary(
                '/request.KarlController/Heartbeat',
                request_serializer=request__pb2.HostHeartbeat.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.SensorRegister = channel.unary_unary(
                '/request.KarlController/SensorRegister',
                request_serializer=request__pb2.SensorRegisterRequest.SerializeToString,
                response_deserializer=request__pb2.SensorRegisterResult.FromString,
                )
        self.PushRawData = channel.unary_unary(
                '/request.KarlController/PushRawData',
                request_serializer=request__pb2.SensorPushData.SerializeToString,
                response_deserializer=google_dot_protobuf_dot_empty__pb2.Empty.FromString,
                )
        self.StateChanges = channel.unary_stream(
                '/request.KarlController/StateChanges',
                request_serializer=request__pb2.StateChangeInit.SerializeToString,
                response_deserializer=request__pb2.StateChangePair.FromString,
                )


class KarlControllerServicer(object):
    """Missing associated documentation comment in .proto file."""

    def HostRegister(self, request, context):
        """hosts
        """
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def ForwardNetwork(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def ForwardGet(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def ForwardPush(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def ForwardState(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def FinishCompute(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def Heartbeat(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def SensorRegister(self, request, context):
        """sensors
        """
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def PushRawData(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')

    def StateChanges(self, request, context):
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details('Method not implemented!')
        raise NotImplementedError('Method not implemented!')


def add_KarlControllerServicer_to_server(servicer, server):
    rpc_method_handlers = {
            'HostRegister': grpc.unary_unary_rpc_method_handler(
                    servicer.HostRegister,
                    request_deserializer=request__pb2.HostRegisterRequest.FromString,
                    response_serializer=request__pb2.HostRegisterResult.SerializeToString,
            ),
            'ForwardNetwork': grpc.unary_unary_rpc_method_handler(
                    servicer.ForwardNetwork,
                    request_deserializer=request__pb2.NetworkAccess.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'ForwardGet': grpc.unary_unary_rpc_method_handler(
                    servicer.ForwardGet,
                    request_deserializer=request__pb2.GetData.FromString,
                    response_serializer=request__pb2.GetDataResult.SerializeToString,
            ),
            'ForwardPush': grpc.unary_unary_rpc_method_handler(
                    servicer.ForwardPush,
                    request_deserializer=request__pb2.PushData.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'ForwardState': grpc.unary_unary_rpc_method_handler(
                    servicer.ForwardState,
                    request_deserializer=request__pb2.StateChange.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'FinishCompute': grpc.unary_unary_rpc_method_handler(
                    servicer.FinishCompute,
                    request_deserializer=request__pb2.NotifyEnd.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'Heartbeat': grpc.unary_unary_rpc_method_handler(
                    servicer.Heartbeat,
                    request_deserializer=request__pb2.HostHeartbeat.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'SensorRegister': grpc.unary_unary_rpc_method_handler(
                    servicer.SensorRegister,
                    request_deserializer=request__pb2.SensorRegisterRequest.FromString,
                    response_serializer=request__pb2.SensorRegisterResult.SerializeToString,
            ),
            'PushRawData': grpc.unary_unary_rpc_method_handler(
                    servicer.PushRawData,
                    request_deserializer=request__pb2.SensorPushData.FromString,
                    response_serializer=google_dot_protobuf_dot_empty__pb2.Empty.SerializeToString,
            ),
            'StateChanges': grpc.unary_stream_rpc_method_handler(
                    servicer.StateChanges,
                    request_deserializer=request__pb2.StateChangeInit.FromString,
                    response_serializer=request__pb2.StateChangePair.SerializeToString,
            ),
    }
    generic_handler = grpc.method_handlers_generic_handler(
            'request.KarlController', rpc_method_handlers)
    server.add_generic_rpc_handlers((generic_handler,))


 # This class is part of an EXPERIMENTAL API.
class KarlController(object):
    """Missing associated documentation comment in .proto file."""

    @staticmethod
    def HostRegister(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/HostRegister',
            request__pb2.HostRegisterRequest.SerializeToString,
            request__pb2.HostRegisterResult.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def ForwardNetwork(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/ForwardNetwork',
            request__pb2.NetworkAccess.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def ForwardGet(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/ForwardGet',
            request__pb2.GetData.SerializeToString,
            request__pb2.GetDataResult.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def ForwardPush(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/ForwardPush',
            request__pb2.PushData.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def ForwardState(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/ForwardState',
            request__pb2.StateChange.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def FinishCompute(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/FinishCompute',
            request__pb2.NotifyEnd.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def Heartbeat(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/Heartbeat',
            request__pb2.HostHeartbeat.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def SensorRegister(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/SensorRegister',
            request__pb2.SensorRegisterRequest.SerializeToString,
            request__pb2.SensorRegisterResult.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def PushRawData(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_unary(request, target, '/request.KarlController/PushRawData',
            request__pb2.SensorPushData.SerializeToString,
            google_dot_protobuf_dot_empty__pb2.Empty.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

    @staticmethod
    def StateChanges(request,
            target,
            options=(),
            channel_credentials=None,
            call_credentials=None,
            insecure=False,
            compression=None,
            wait_for_ready=None,
            timeout=None,
            metadata=None):
        return grpc.experimental.unary_stream(request, target, '/request.KarlController/StateChanges',
            request__pb2.StateChangeInit.SerializeToString,
            request__pb2.StateChangePair.FromString,
            options, channel_credentials,
            insecure, call_credentials, compression, wait_for_ready, timeout, metadata)

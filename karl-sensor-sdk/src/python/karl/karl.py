import grpc

from . import request_pb2
from . import request_pb2_grpc

class KarlSensor:
	"""
	Object representing a KarlSensor
	"""
	
	def __init__(self, channel, token=None):
		"""
		Constructor for KarlSensor

		Parameters
		----------
		channel : gRPC.Channel
			gRPC channel representing connection to controller
		token : str
			Sensor token (for already registered sensors)
		"""
	
		self.sensor_token = token
		self.stub = request_pb2_grpc.KarlControllerStub(channel)

	def register(self, global_sensor_id, keys, returns, app=[]):
		registerRequest = request_pb2.SensorRegisterRequest(global_sensor_id=global_sensor_id,keys=keys,returns=returns,app=app)
		response = self.stub.SensorRegister(registerRequest)
		self.sensor_token = response.sensor_token;
		return response;

	def push(self, param, data):
		if not self.sensor_token:
			raise ValueError("Sensor token is not defined, instiantiate with token or call register()")
		else:
			pushRequest = request_pb2.SensorPushData(sensor_token = self.sensor_token, param = param, data = data)
			response = self.stub.PushRawData(pushRequest)
			return response
	
	def connectState(self):
		if not self.sensor_token:
			raise ValueError("Sensor token is not defined, instiantiate with token or call register()")
		else:
			stateRequest = request_pb2.StateChangeInit(sensor_token = self.sensor_token)
			pairs = self.stub.StateChanges(stateRequest)
			return pairs

#include "request.pb.h"
#include "request.grpc.pb.h"
#include <iostream>
#include <vector>
#include <grpcpp/grpcpp.h>
#include "Karl.h"

using namespace std;
using request::SensorRegisterResult;
using request::KarlController;
using request::SensorPushData;
using request::SensorRegisterRequest;
using request::StateChangeInit;
using request::StateChangePair;
using grpc::Channel;
using grpc::ClientContext;
using grpc::ClientReader;
using grpc::Status;

KarlSensorSDK::KarlSensorSDK(std::shared_ptr<Channel> channel): stub_(KarlController::NewStub(channel)){};

KarlSensorSDK::KarlSensorSDK(string token,std::shared_ptr<Channel> channel): stub_(KarlController::NewStub(channel)){
	this->sensor_token = token;
}

SensorRegisterResult KarlSensorSDK::sensorRegister(string global_sensor_id, vector<string>& keys, vector<string>& returns, vector<int>& app) {
	
	SensorRegisterRequest request;
	request.set_global_sensor_id(global_sensor_id);
	*request.mutable_keys() = {keys.begin(), keys.end()};
	*request.mutable_returns() = {returns.begin(), returns.end()};
	*request.mutable_app() = {app.begin(), app.end()};
	
	SensorRegisterResult response;
	ClientContext context;
	Status status = stub_->SensorRegister(&context,request,&response);

	if (!status.ok()) 
		cerr << status.error_code() << ": " << status.error_message() << endl;
	
	this->sensor_token = response.sensor_token();	
	return response;
}

void KarlSensorSDK::push(string param, vector<char>& data) {
	SensorPushData request;
	request.set_sensor_token(this->sensor_token);
	request.set_param(param);
	*request.mutable_data() = {data.begin(), data.end()};
	
	ClientContext context;
 	google::protobuf::Empty response;
	Status status = stub_->PushRawData(&context,request,&response);
	
	if (!status.ok()) 
		cerr << status.error_code() << ": " << status.error_message() << endl;	
}


unique_ptr<ClientReader<StateChangePair>> KarlSensorSDK::connectState() {
	
	StateChangeInit request;
	request.set_sensor_token(this->sensor_token);
	
	StateChangePair pair;		
	ClientContext context;
	unique_ptr<ClientReader<StateChangePair>> reader (stub_->StateChanges(&context,request));
	
	return reader;
}

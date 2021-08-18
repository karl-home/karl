#pragma once
#include "request.pb.h"
#include "request.grpc.pb.h"
#include <iostream>
#include <vector>
#include <grpcpp/grpcpp.h>

using namespace std;
using request::KarlController;
using request::SensorRegisterResult;
using request::StateChangePair;
using grpc::Channel;
using grpc::ClientReader;
using grpc::Status;

class KarlSensorSDK {
	public:
		string sensor_token;
	    
	    KarlSensorSDK(std::shared_ptr<Channel> channel);
	    KarlSensorSDK(string token,std::shared_ptr<Channel> channel);
	    
	    SensorRegisterResult sensorRegister(string global_sensor_id, vector<string> keys, vector<string> returns, vector<int> app);
	    void push(string param, vector<char> data);
		unique_ptr<ClientReader<StateChangePair>> connectState();
        
	private:
		unique_ptr<KarlController::Stub> stub_;
};

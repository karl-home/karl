#include <grpcpp/grpcpp.h>
#include "request.pb.h"
#include "request.grpc.pb.h"
#include <Karl.h>
#include <thread>
#include <iostream>
#include <chrono>    
#include <fstream>

void handle_state_changes(unique_ptr<ClientReader<StateChangePair>> reader) {
	StateChangePair pair;
	while(reader->Read(&pair)) {
		if(pair.key() == "firmware") {
			std::cout << "firmware update!" << std::endl;
		}
		else if(pair.key() == "livestream") {
			if(pair.value()[0] == 1) {
				std::cout << "turning livestream on" << std::endl;
			}
			else {
				std::cout << "turning livestream off" << std::endl;
			}
		}
		else {
			std::cerr << "warning, unexpected key: " << pair.key() << std::endl;
		}
	}
}

void motion_detection(KarlSensorSDK &karl, vector<char>& fileData) {
	while(true) {
		std::cout << "starting to push" << std::endl;
		karl.push("motion",fileData);
		std::cout << "finished push" << std::endl;
		std::this_thread::sleep_for(std::chrono::seconds(30));
	}
}

int main() {
	std::cout << "hi there" << std::endl;
	KarlSensorSDK karl(
      grpc::CreateChannel("localhost:59582", grpc::InsecureChannelCredentials()));
	request::SensorRegisterResult result = karl.sensorRegister("camera", {"firmware","livestream"}, {"motion","streaming"}, {});
	std::cout << "Registered camera with sensor_token " << result.sensor_token() << " and sensor_id " << result.sensor_id() << std::endl;
	
	std::ifstream infile("../../../../data/FudanPed00001.png");
	infile.seekg(0, std::ios::end);
	size_t length = infile.tellg();
	infile.seekg(0,std::ios::beg);

	vector<char> fileData;
	fileData.resize(length);
	infile.seekg(0, std::ios_base::beg);
    infile.read(&fileData[0], length);
	std::thread t1(motion_detection, std::ref(karl), std::ref(fileData));
	t1.join();
	
	return 0;
}


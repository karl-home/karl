// Register.cpp

#include <fstream>
#include <chrono>
#include "Karl.h"

int main(int argc, char *argv[])
{
    GOOGLE_PROTOBUF_VERIFY_VERSION;

    int CONTROLLER_PORT;
    const char* CONTROLLER_IP;
    const char* CLIENT_ID = "wyzecam";

    const char* app_file;
    if (argc > 1) {
        app_file = argv[1];
    } else {
        app_file = "data/index.hbs";
    }
    if (argc > 2) {
        CONTROLLER_IP = argv[2];
    } else {
        CONTROLLER_IP = "192.168.1.5";
    }
    if (argc > 3) {
        CONTROLLER_PORT = atoi(argv[3]);
    } else {
        CONTROLLER_PORT = 59582;
    }
    auto t1 = std::chrono::high_resolution_clock::now();
    std::ifstream buffer(app_file, std::ios::binary);
    std::vector<char> app_bytes(std::istreambuf_iterator<char>(buffer), {});

    RegisterResult result = Karl::Net::RegisterClient(
        CONTROLLER_PORT, CONTROLLER_IP, CLIENT_ID,
        app_bytes.data(), app_bytes.size());
    auto t2 = std::chrono::high_resolution_clock::now();
    cout << "export KARL_CLIENT_TOKEN=" << result.client_token() << endl;
    cerr << "execute the export command before running PersonDetection.cpp" << endl;
    cerr << "registerClient:" << std::chrono::duration_cast<std::chrono::microseconds>( t2 - t1 ).count() << endl;

    google::protobuf::ShutdownProtobufLibrary();

    return 0;
}

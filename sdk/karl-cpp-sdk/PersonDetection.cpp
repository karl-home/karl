// PersonDetection.cpp

#include <iostream>
#include <chrono>
#include "Karl.h"

using namespace std;
using namespace request;

int main(int argc, char *argv[])
{
    int CONTROLLER_PORT;
    const char* CONTROLLER_IP;
    const char* CLIENT_ID = "wyzecam";
    const char* CLIENT_TOKEN = getenv("KARL_CLIENT_TOKEN");

    if (!CLIENT_TOKEN) {
        cerr << "set KARL_CLIENT_TOKEN environment variable" << endl;
        return -1;
    }
    cerr << "client token = " << CLIENT_TOKEN << endl;

    const char* img_path;
    const char* py_path;
    if (argc > 1) {
        py_path = argv[1];
    } else {
        py_path = "data/detect.py";
    }
    if (argc > 2) {
        img_path = argv[2];
    } else {
        img_path = "data/img.tmp";
    }
    if (argc > 3) {
        CONTROLLER_IP = argv[3];
    } else {
        CONTROLLER_IP = "192.168.1.5";
    }
    if (argc > 4) {
        CONTROLLER_PORT = atoi(argv[4]);
    } else {
        CONTROLLER_PORT = 59582;
    }

    const char* dst_img_path = "data/img.tmp";
    const string args[] = {"storage/detect.py", dst_img_path};

    auto t1 = std::chrono::high_resolution_clock::now();
    Karl::ComputeRequestBuilder builder = Karl::ComputeRequestBuilder("storage/env/bin/python");
    builder.Args(args, 2);
    builder.AddFileAs(img_path, dst_img_path);
    request::ComputeRequest req = builder.Finalize();
    req.set_stdout(true);
    req.set_storage(true);
    req.set_client_id(CLIENT_ID);

    auto t2 = std::chrono::high_resolution_clock::now();
    ComputeResult result = Karl::Net::SendComputeBlocking(
        CONTROLLER_PORT, CONTROLLER_IP, CLIENT_TOKEN, req);
    auto t3 = std::chrono::high_resolution_clock::now();
    cout << result.stdout() << endl;

    cerr << "buildRequest:" << std::chrono::duration_cast<std::chrono::microseconds>( t2 - t1 ).count() / 1000000. << endl;
    cerr << "SendComputeBlocking:" << std::chrono::duration_cast<std::chrono::microseconds>( t3 - t2 ).count() / 1000000. << endl;
    cerr << "total:" << std::chrono::duration_cast<std::chrono::microseconds>( t3 - t1 ).count() / 1000000. << endl;

    return 0;
}

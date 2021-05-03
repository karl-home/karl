// Karl.h
#pragma once
#pragma GCC diagnostic ignored "-Wexpansion-to-defined"

#include "protos/request.pb.h"

using namespace std;
using namespace request;

namespace Karl
{
    class Net
    {
    public:
        /**
         * Register an IoT client with the controller.
         *
         * If the client wants to register a web app, it needs to include
         * the bytes of a single Handlebars template file.
         */
        static RegisterResult RegisterClient(int portno, const char *ip, string id, char* app_bytes, size_t app_length);

        /**
         * Returns a host address given by the controller.
         */
        static HostResult GetHost(int portno, const char *ip, string client_token, bool blocking);

        /**
         * Sends a compute request to the given host and returns the result.
         *
         * Result is null if the host did not accept the request. There may be
         * an issue with the request, or the client should call GetHost and try
         * again with a new request token.
         *
         * Return value indicates success = 0 or failure != 0.
         * Result is written in the given pointer.
         */
        static int SendCompute(int portno, const char *ip, ComputeRequest req, ComputeResult *res);

        /**
         * Like HostRequest and SendCompute combined.
         *
         * Reserves a host in a blocking request and attemps to send a
         * compute request. Retries until the request succeeds.
         */
        static ComputeResult SendComputeBlocking(
            int cportno, const char *cip,
            string client_token, ComputeRequest req);
    };

    class ComputeRequestBuilder
    {
    private:
        vector<tuple<string, string>> dirs;
        vector<tuple<string, string>> files;
        vector<request::Import> imports;
        PkgConfig config;
    public:
        ComputeRequestBuilder(string path);

        /**
         * Arguments, not including the binary path.
         */
        void Args(const string* args, size_t size);

        /**
         * Environment variables in the format <ENV_VARIABLE>=<VALUE>.
         */
        void Envs(const string* envs, size_t size);

        /**
         * Import external library or package.
         */
        void Import(string name);

        /**
         * Same as `AddFileAs`, but uses the src_path as the dst_path.
         */
        void AddFile(string path);

        /**
         * Same as `AddDirAs`, but uses the src_path as the dst_path.
         */
        void AddDir(string path);

        /**
         * Add a file to the input root from the home filesystem,
         * overwriting files with the same name.
         */
        void AddFileAs(string src_path, string dst_path);

        /**
         * Add a directory to the input root from the home filesystem,
         * overwriting files with the same name.
         */
        void AddDirAs(string src_path, string dst_path);

        /**
         * Finalize the compute request.
         */
        ComputeRequest Finalize();
    };
}

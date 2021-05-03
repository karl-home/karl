// Karl.cpp

#include <fstream>
#include <sstream>
#include <iterator>
#include <vector>
#include <array>
#include <unistd.h>
#include <netdb.h>
#include "Karl.h"

using namespace std;
using namespace request;

namespace Karl
{
    static size_t HEADER_LEN = 8;

    void error(const char *msg)
    {
        perror(msg);
        exit(0);
    }

    void kassert(bool res, const char *msg)
    {
        if (!res) cerr << msg << endl;
    }

    int kassert_cerr(bool res, const char *msg)
    {
        if (!res) cerr << msg << endl;
        return res ? 0 : 1;
    }

    vector<char> exec(string input_cmd) {
        string cmd, out;
        out = "/tmp/person-detection.tmp";
        cmd += input_cmd;
        cmd += " > ";
        cmd += out;
        system(cmd.c_str());
        ifstream input(out, ios::binary);
        vector<char> buffer(std::istreambuf_iterator<char>(input), {});
        return buffer;
    }

    /**
     * Returns file descriptor for a TCP connection to the given port and ip.
     * The client is responsible for closing the file descriptor after use.
     *
     * http://www.linuxhowtos.org/C_C++/socket.htm
     */
    int connect(int portno, const char *ip)
    {
        int sockfd;
        struct sockaddr_in serv_addr;
        struct hostent *server;

        sockfd = socket(AF_INET, SOCK_STREAM, 0);
        if (sockfd < 0)
            error("ERROR opening socket");

        server = gethostbyname(ip);
        bzero((char *) &serv_addr, sizeof(serv_addr));
        serv_addr.sin_family = AF_INET;
        bcopy((char *)server->h_addr,
              (char *)&serv_addr.sin_addr.s_addr,
              server->h_length);
        serv_addr.sin_port = htons(portno);

        // connect to the server
        if (connect(sockfd, (struct sockaddr *) &serv_addr, sizeof(serv_addr)) < 0)
            error("ERROR conecting");
        return sockfd;
    }

    /**
     * Send a message to the socket at the file descriptor.
     */
    void send_msg(int sockfd, MessageType ty, int length, const char* buffer)
    {
        int n;
        char header[HEADER_LEN];

        // https://stackoverflow.com/questions/1522994/store-an-int-in-a-char-array
        kassert(sizeof(ty) == 4, "MakeHeader expected sizeof(ty) == 4");
        kassert(sizeof(length) == 4, "MakeHeader expected sizeof(length) == 4");
        memcpy(header, &ty, 4);
        memcpy(header + 4, &length, 4);

        n = write(sockfd, header, HEADER_LEN);
        kassert(n == HEADER_LEN, "ERROR writing header");
        cerr << "send header (" << HEADER_LEN << " bytes)" << endl;
        n = write(sockfd, buffer, length);
        cerr << "send message (" << length << " bytes)" << endl;
        kassert(n == length, "ERROR writing message");
    }

    /**
     * Read a message from the socket at the file descriptor with the
     * expected message type.
     */
    vector<char> recv_msg(int sockfd, MessageType expected_ty)
    {
        int n, size;
        char header[HEADER_LEN];
        MessageType ty;

        n = read(sockfd, header, HEADER_LEN);
        if (kassert_cerr(!(n < 0), "ERROR reading bytes"))
            return vector<char>();
        if (kassert_cerr(n == HEADER_LEN, "ERROR not enough bytes to fill header"))
            return vector<char>();
        cerr << "read header (" << n << " bytes)" << endl;
        memcpy(&ty, header, 4);
        memcpy(&size, header + 4, 4);
        if (ty != expected_ty) {
            cerr << "ty " << ty << " != " << expected_ty << endl;
            cerr << "size = " << size << endl;
            error("ReadHeader expected different message type");
        }

        vector<char> v(size);
        n = read(sockfd, v.data(), size);
        if (kassert_cerr(!(n < 0), "ERROR reading bytes"))
            return vector<char>();
        if (kassert_cerr(n == size, "ERROR reading message"))
            return vector<char>();
        cerr << "read message (" << n << " bytes)" << endl;
        return v;
    }

    RegisterResult Net::RegisterClient(int portno, const char *ip, string id, char* app_bytes, size_t app_length)
    {
        int sockfd;
        string buffer;
        vector<char> v;
        RegisterRequest req;
        RegisterResult res;

        sockfd = connect(portno, ip);
        req.set_id(id);
        if (app_length > 0) {
            req.set_app(app_bytes, app_length);
        }
        req.AppendToString(&buffer);
        send_msg(sockfd, MessageType::REGISTER_REQUEST, buffer.size(), buffer.c_str());
        v = recv_msg(sockfd, MessageType::REGISTER_RESULT);
        kassert(res.ParseFromString(v.data()), "ERROR deserializing message");
        close(sockfd);
        return res;
    }

    HostResult Net::GetHost(int portno, const char *ip, string client_token, bool blocking)
    {
        int sockfd;
        string buffer;
        vector<char> v;
        HostRequest req;
        HostResult res;

        sockfd = connect(portno, ip);
        req.set_client_token(client_token);
        req.set_blocking(blocking);
        req.AppendToString(&buffer);
        send_msg(sockfd, MessageType::HOST_REQUEST, buffer.size(), buffer.c_str());
        v = recv_msg(sockfd, MessageType::HOST_RESULT);
        kassert(res.ParseFromString(v.data()), "ERROR deserializing message");
        close(sockfd);
        return res;
    }

    int Net::SendCompute(int portno, const char *ip, ComputeRequest req, ComputeResult* res)
    {
        int sockfd;
        string buffer;
        vector<char> v;

        sockfd = connect(portno, ip);
        req.AppendToString(&buffer);
        send_msg(sockfd, MessageType::COMPUTE_REQUEST, buffer.size(), buffer.c_str());
        buffer.clear();
        v = recv_msg(sockfd, MessageType::COMPUTE_RESULT);
        if (v.empty()) return -1;

        // need to use istream instead of stream because expecting raw bytes
        string s((char*) v.data(), v.size());
        istringstream iss(s);
        kassert(res->ParseFromIstream(&iss), "ERROR deserializing message");
        close(sockfd);
        return 0;
    }

    ComputeResult Net::SendComputeBlocking(
        int cportno, const char *cip,
        string client_token, ComputeRequest req)
    {
        int val;
        ComputeResult res;
        while (true)
        {
            HostResult host = Karl::Net::GetHost(cportno, cip, client_token, true);
            req.set_request_token(host.request_token().c_str());
            cerr << "host = " << host.ip() << ":" << host.port() << endl;
            val = Karl::Net::SendCompute(host.port(), host.ip().c_str(), req, &res);
            cerr << "send compute" << endl;
            if (val == 0)
                return res;
        }
    }

    ComputeRequestBuilder::ComputeRequestBuilder(string path)
    {
        config.set_binary_path(path.c_str());
    }

    void ComputeRequestBuilder::Args(const string* args, size_t size)
    {
        config.clear_args();
        for (int i = 0; i < size; i++) {
            config.add_args(args[i]);
        }
    }

    void ComputeRequestBuilder::Envs(const string* envs, size_t size)
    {
        config.clear_envs();
        for (int i = 0; i < size; i++) {
            config.add_envs(envs[i]);
        }
    }

    void ComputeRequestBuilder::Import(string name)
    {
        request::Import imp;
        imp.set_name(name);
        imp.set_hash("TODO");
        imports.push_back(imp);
    }

    void ComputeRequestBuilder::AddFile(string path)
    {
        AddFileAs(path, path);
    }

    void ComputeRequestBuilder::AddDir(string path)
    {
        AddDirAs(path, path);
    }

    void ComputeRequestBuilder::AddFileAs(string src_path, string dst_path)
    {
        files.push_back(make_tuple(src_path, dst_path));
    }

    void ComputeRequestBuilder::AddDirAs(string src_path, string dst_path)
    {
        dirs.push_back(make_tuple(src_path, dst_path));
    }

    ComputeRequest ComputeRequestBuilder::Finalize()
    {
        int i;
        vector<char> package;
        ComputeRequest req;
        const char* tar_path;

        if (files.size() > 0 || dirs.size() > 0)
        {
            string cmd, tfs, paths, src_path, dst_path;
            for (i = 0; i < files.size(); i++)
            {
                std::tie(src_path, dst_path) = files[i];
                paths += " " + src_path;
                // Remove leading "/" from src_path in transform.
                if (src_path.compare(dst_path) == 0)
                    continue;
                if (src_path[0] == '/')
                    src_path.erase(0, 1);
                tfs += " --transform \"s|" + src_path + "|" + dst_path + "|\"";
            }
            for (i = 0; i < dirs.size(); i++)
            {
                std::tie(src_path, dst_path) = dirs[i];
                paths += " " + src_path;
                // Remove leading "/" from src_path in transform.
                if (src_path.compare(dst_path) == 0)
                    continue;
                if (src_path[0] == '/')
                    src_path.erase(0, 1);
                tfs += " --transform \"s|" + src_path + "|" + dst_path + "|\"";
            }
            if ((tar_path = getenv("KARL_TAR_PATH"))) {
                cerr << "Using tar path = " << tar_path << endl;
                cmd = tar_path;
            } else {
                cmd = "tar";
            }
            cmd += " cz" + paths + tfs;
            package = exec(cmd);
            cerr << cmd << ": " << package.size() << " bytes" << endl;
        }
        req.set_package(package.data(), package.size());
        *req.mutable_config() = config;
        for (i = 0; i < imports.size(); i++)
        {
            *req.add_imports() = imports[i];
        }
        cerr << req.imports_size() << " imports" << endl;
        return req;
    }
}

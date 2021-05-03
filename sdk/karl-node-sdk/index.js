const net = require('net');
const tar = require('tar');
const karl = require('./protos/request_pb.js');

exports.HostRequest = karl.HostRequest;
exports.HostResult = karl.HostResult;
exports.ComputeRequest = karl.ComputeRequest;
exports.ComputeResult = karl.ComputeResult;

const HEADER_LENGTH = 8;

function concatBuffers(data) {
    let length = 0;
    for (let buffer of data) {
        length += buffer.length;
    }
    return Buffer.concat(data, length);
}

function makeHeader(ty, length) {
    let header = new ArrayBuffer(HEADER_LENGTH);
    if (ty >= 4294967296)
        throw "invalid message type";
    if (length >= 4294967296)
        throw "too long of a message";
    let view = new DataView(header);
    view.setUint32(0, ty, true);
    view.setUint32(4, length, true);
    // console.log('makeHeader:', header);
    return header;
}

function readHeader(buffer) {
    if (buffer.length < HEADER_LENGTH)
        throw "buffer too short to have a header";
    let header = new Uint8Array(buffer.slice(0, HEADER_LENGTH));
    let view = new DataView(header.buffer);
    // console.log('readHeader:', header);
    let ty = view.getUint32(0, true);
    let length = view.getUint32(4, true);
    return { ty: ty, length: length };
}

/**
 * Register an IoT client with the controller.
 *
 * If the client wants to register a web app, it needs to include the bytes
 * of a single Handlebars template file.
 */
function registerClient(port, ip, id, appBytes) {
    return new Promise(function(resolve, reject) {
        let conn = new net.Socket();
        let buffer = [];
        conn.on('error', function(e) {
            console.error(e);
            reject(e);
        });
        conn.on('connect', function() {
            // send a host request to the controller
            let req = new karl.RegisterRequest();
            req.setId(id);
            if (appBytes) {
                req.setAppBytes(appBytes);
            }
            let bytes = req.serializeBinary();
            let header = makeHeader(karl.MessageType.REGISTER_REQUEST, bytes.length);
            conn.write(new Buffer(header));
            conn.write(bytes);
        });
        conn.on('data', function(data) {
            buffer.push(data);
        });
        conn.on('close', function() {
            let bytes = concatBuffers(buffer);
            let header = readHeader(bytes.slice(0, HEADER_LENGTH));
            bytes = bytes.slice(HEADER_LENGTH);
            if (header.ty != karl.MessageType.REGISTER_RESULT)
                throw "HostResult header is wrong type";
            if (header.length != bytes.length)
                throw "header length does not match";
            let result = karl.RegisterResult.deserializeBinary(bytes);
            resolve(result);
        });
        conn.connect(port, ip);
    });
}
exports.registerClient = registerClient;

/**
 * Send a host request to the controller and return the result.
 *
 * @param port: controller port.
 * @param ip: controller ip.
 * @param clientToken: the client token.
 * @param blocking: whether the request should be blocking.
 * @returns HostResult
 */
function getHost(port, ip, clientToken, blocking) {
    return new Promise(function(resolve, reject) {
        let conn = new net.Socket();
        let buffer = [];
        conn.on('error', function(e) {
            console.error(e);
            reject(e);
        });
        conn.on('connect', function() {
            // send a host request to the controller
            let req = new karl.HostRequest();
            req.setClientToken(clientToken);
            req.setBlocking(blocking ? true : false);
            let bytes = req.serializeBinary();
            let header = makeHeader(karl.MessageType.HOST_REQUEST, bytes.length);
            conn.write(new Buffer(header));
            conn.write(bytes);
        });
        conn.on('data', function(data) {
            buffer.push(data);
        });
        conn.on('close', function() {
            let bytes = concatBuffers(buffer);
            let header = readHeader(bytes.slice(0, HEADER_LENGTH));
            bytes = bytes.slice(HEADER_LENGTH);
            if (header.ty != karl.MessageType.HOST_RESULT)
                throw "HostResult header is wrong type";
            if (header.length != bytes.length)
                throw "header length does not match";
            let result = karl.HostResult.deserializeBinary(bytes);
            console.log('getHost:', result.array);
            resolve(result);
        });
        conn.connect(port, ip);
    });
}
exports.getHost = getHost;

/**
 * Send a compute request to the given host and returns the result.
 *
 * @param hostPort: the host port, probably obtained from the controller.
 * @param hostIp: the host ip, probably obtained from the controller.
 * @param request: the request following the representation in the Rust crate.
 * @returns ComputeResult
 */
function sendCompute(hostPort, hostIp, req) {
    return new Promise(function(resolve, reject) {
        let conn = new net.Socket();
        let buffer = [];
        conn.on('connect', function() {
            let bytes = req.serializeBinary();
            let header = makeHeader(karl.MessageType.COMPUTE_REQUEST, bytes.length);
            conn.write(new Buffer(header));
            conn.write(bytes);
        });
        conn.on('error', function(e) {
            console.error(e);
            reject(e);
        });
        conn.on('data', function(data) {
            buffer.push(data);
        });
        conn.on('close', function() {
            let bytes = concatBuffers(buffer);
            let header = readHeader(bytes.slice(0, HEADER_LENGTH));
            bytes = bytes.slice(HEADER_LENGTH);
            if (header.ty != karl.MessageType.COMPUTE_RESULT)
                throw "ComputeResult header is wrong type";
            if (header.length != bytes.length)
                throw "header length does not match";
            let result = karl.ComputeResult.deserializeBinary(bytes);
            console.log('sendCompute:', result.array);
            resolve(result);
        });
        conn.connect(hostPort, hostIp);
    });
}
exports.sendCompute = sendCompute;

/**
 * Like HostRequest and SendCompute combined.
 *
 * Reserves a host in a blocking request and attempts to send a
 * compute request. Retries until the request succeeds.
 *
 * WARNING: fails to consider if compute requests fail because the
 * request is malformed, not because the request token expired.
 * Actually, fails to consider if compute request fails for any reason...
 *
 * @param controllerPort: the controller port.
 * @param controllerIp: the controller ip.
 * @param clientToken: the client token.
 * @param request: the request following the representation in the Rust crate.
 * @returns ComputeResult
 */
function sendComputeBlocking(controllerPort, controllerIp, clientToken, req) {
    return new Promise(function(resolve, reject) {
        getHost(controllerPort, controllerIp, clientToken, true)
        .then(function(host) {
            console.error(`host = ${host.getIp()}:${host.getPort()}`);
            req.setRequestToken(host.getRequestToken());
            return sendCompute(host.getPort(), host.getIp(), req);
        })
        .then(function(result) { resolve(result); })
        .catch(function (e) { reject(e); });
    });
}
exports.sendComputeBlocking = sendComputeBlocking;

class ComputeRequestBuilder {
    constructor(binaryPath) {
        this.dirs = [];
        this.files = [];
        this.imports = [];
        this.config = new karl.PkgConfig();
        this.config.setBinaryPath(binaryPath);
    }

    /**
     * Arguments, not including the binary path.
     *
     * @param args: array of arguments
     */
    args(args) {
        this.config.setArgsList(args);
    }

    /**
     * Environment variables in the format <ENV_VARIABLE>=<VALUE>.
     *
     * @param envs: array of environment variables
     */
    envs(envs) {
        this.config.setEnvsList(args);
    }

    /**
     * Import external library or package. Local imports only.
     *
     * @param name: name of the import
     */
    import(name) {
        let imp = new karl.Import();
        imp.setName(name);
        imp.setHash("TODO");
        this.imports.push(imp);
    }

    /**
     * Add a file to the input root from the home filesystem, overwriting
     * files with the same name.
     *
     * @param path: the file name
     */
    add_file(path) {
        this.files.push(path);
    }

    /**
     * Add a directory to the input root from the home filesystem, overwriting
     * files with the same name.
     * TODO: needs to be a relative path. should be forward searching only.
     *
     * @param path: the file name
     */
    add_dir(path) {
        this.dirs.push(path);
    }

    /**
     * Finalize the compute request.
     *
     * @param callback: callback that takes the compute request as a parameter
     */
    finalize() {
        let res = tar.create({
            gzip: true,
            sync: true,
            follow: true,
        }, this.dirs.concat(this.files));
        let req = new karl.ComputeRequest();
        req.setPackage(res.read());
        req.setConfig(this.config);
        req.setImportsList(this.imports);
        return req;
    }
}
exports.ComputeRequestBuilder = ComputeRequestBuilder;

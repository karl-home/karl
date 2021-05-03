# MAKE A MAKEFILE???
/home/gina/mipsel/bin/mipsel-linux-g++ -std=c++11 -muclibc -O2 -lrt \
	-Wno-expansion-to-defined Register.cpp Karl.cpp protos/request.pb.cc -o register \
	-pthread -I/home/gina/proto-mips/include -L/home/gina/proto-mips/lib \
	-lprotobuf -latomic -static -s
/home/gina/mipsel/bin/mipsel-linux-g++ -std=c++11 -muclibc -O2 -lrt \
	-Wno-expansion-to-defined PersonDetection.cpp Karl.cpp protos/request.pb.cc -o person_detection \
	-pthread -I/home/gina/proto-mips/include -L/home/gina/proto-mips/lib \
	-lprotobuf -latomic -static -s
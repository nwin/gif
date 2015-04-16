CFLAGS = -I../..
LD_LIBRARY_PATH = ../../target/release
LDFLAGS = -L${LD_LIBRARY_PATH}
LDLIBS = -lgif
CC = cc

read_gif: read_gif.c ${LD_LIBRARY_PATH}/libgif.dylib
	$(CC) -O2 $(LDLIBS) $(LDFLAGS) $(CFLAGS) -o read_gif read_gif.c

${LD_LIBRARY_PATH}/libgif.dylib: 
	(cd ../../ && cargo build --release)
	cp ${LD_LIBRARY_PATH}/libgif*.dylib ${LD_LIBRARY_PATH}/libgif.dylib
	
clean:
	rm read_gif

run:
	./read_gif ../../tests/samples/sample_1.gif
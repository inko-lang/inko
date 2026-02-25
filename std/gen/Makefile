MIME_JSON := data/mime.json
MIME_SRC := ../src/std/mime

mime:
	mkdir -p ${MIME_SRC}
	jq 'keys[]' --raw-output ${MIME_JSON} > /tmp/keys.txt
	python hash.py /tmp/keys.txt template.tmpl --hft 2 --trials 200 --pow2 \
		--output std | inko fmt - > ${MIME_SRC}/phf.inko
	rm /tmp/keys.txt
	rm -f ${MIME_SRC}/data.inko
	echo 'let TYPES = ' > ${MIME_SRC}/data.inko
	jq '[values[]]' ${MIME_JSON} -c --monochrome-output >> ${MIME_SRC}/data.inko
	inko fmt ${MIME_SRC}/data.inko

http_method:
	mkdir -p ../src/std/net/http/method
	inko run http_methods/src/main.inko
	inko fmt ../src/std/net/http/method.inko
	inko fmt ../test/std/net/http/test_method.inko

http_header:
	mkdir -p ../src/std/net/http/header
	python hash.py data/http_headers.txt template.tmpl --hft 2 --trials 200 \
		--pow2 --output std | inko fmt - > ../src/std/net/http/header/phf.inko
	inko run http_headers/src/main.inko
	inko fmt ../src/std/net/http/header.inko
	inko fmt ../test/std/net/http/test_header.inko

.PHONY: mime http_method http_header

MIME_JSON := data/mime.json
MIME_SRC := ../src/std/mime

mime:
	mkdir -p ${MIME_SRC}
	jq 'keys[]' --raw-output ${MIME_JSON} > /tmp/keys.txt
	python hash.py /tmp/keys.txt template.tmpl --hft 2 --trials 200 \
		--output std | inko fmt - > ${MIME_SRC}/phf.inko
	rm /tmp/keys.txt
	rm -f ${MIME_SRC}/data.inko
	echo 'let TYPES = ' > ${MIME_SRC}/data.inko
	jq '[values[]]' ${MIME_JSON} -c --monochrome-output >> ${MIME_SRC}/data.inko
	inko fmt ${MIME_SRC}/data.inko

http_method:
	python hash.py data/http_methods.txt template.tmpl --hft 2 --trials 200 \
		--output std | inko fmt - > ../src/std/net/http/method.inko

http_header:
	mkdir -p ../src/std/net/http/header
	python hash.py data/http_headers.txt template.tmpl --hft 2 --trials 200 \
		--output std | inko fmt - > ../src/std/net/http/header/phf.inko

.PHONY: mime http_method http_header

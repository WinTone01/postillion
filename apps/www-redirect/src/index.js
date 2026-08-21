export default {
  fetch(request) {
    const url = new URL(request.url);
    url.hostname = "postillion.invalid";
    return Response.redirect(url.toString(), 301);
  },
};

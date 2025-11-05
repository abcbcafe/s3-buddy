# s3-buddy
- Takes an s3 url (s3://)
- Generates presigned url at 12h duration
- Generates static short url in route53 (input) -> points to presigned url
- Refreshes the presigned url before it expires
- Assumes .aws/credentials exist

import oauth_jwt4token
import requests
import json
import os
import sys

CREDENTIAL_FILE = sys.argv[1]
RUN_SERVICE_URL = os.getenv('CLOUD_RUN_SERVICE_URL')

token = oauth_jwt4token.get_id_token(CREDENTIAL_FILE, RUN_SERVICE_URL)
print("\n\nToken is: ", token)

"""
request = requests.get(
    url = RUN_SERVICE_URL,
    headers = {
        'Authorization': f'Bearer {token}'
    }
)

print("Request.json is: ", request.json())

results = {
    'status_code': request.status_code,
    'response': request.json()
}

print(json.dumps(results, indent=2))
"""
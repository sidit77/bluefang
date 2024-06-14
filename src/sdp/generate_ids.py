from urllib.request import urlopen
import yaml
import re


def constify(name: str) -> str:
    name = re.sub('[/]', '', name)
    name = re.sub('IrMC', 'IRMC', name)
    name = re.sub('3D', 'THREED', name)
    words = filter(len, re.split('[- _]', name))
    words = sum([re.findall(r'[A-Z](?:[a-z]+|[A-Z]*(?=[A-Z]|$))', camelcase) for camelcase in words], [])
    # name = re.sub('[- ]', '_', name)
    # name = re.sub('_+', '_', name)
    return "_".join(name.upper() for name in words) 


data = yaml.safe_load(
    urlopen("https://bitbucket.org/bluetooth-SIG/public/raw/HEAD/assigned_numbers/uuids/service_class.yaml").read())
for entry in data['uuids']:
    print("pub const {}: Uuid = Uuid::from_u16({:#04x});".format(constify(entry['name']), entry['uuid']))

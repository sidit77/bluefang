from urllib.request import urlopen
import yaml

data = yaml.safe_load(
    urlopen("https://bitbucket.org/bluetooth-SIG/public/raw/HEAD/assigned_numbers/company_identifiers/company_identifiers.yaml").read())

print("match self.0 {")
for entry in data['company_identifiers']:
    print("    0x{:04x} => Some(\"{}\"),".format(entry['value'], entry['name'].replace('"','\\"')))
print("    _ => None")
print("}")
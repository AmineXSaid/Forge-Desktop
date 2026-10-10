<script lang="ts">
  import Section from '../Section.svelte';
  import Field from '../Field.svelte';
</script>

<Section title="Proxy" caption="Written as environment variables in the scope above.">
  <Field key="env.HTTPS_PROXY" type="env" label="HTTPS proxy" desc="Proxy for HTTPS connections." placeholder="http://proxy:8080" mono />
  <Field key="env.HTTP_PROXY" type="env" label="HTTP proxy" desc="Proxy for HTTP connections." placeholder="http://proxy:8080" mono />
  <Field key="env.NO_PROXY" type="env" label="No proxy" desc="Hosts that skip the proxy, comma-separated." placeholder="localhost,127.0.0.1" mono />
</Section>

<Section title="Certificates" caption="Paths to PEM files. A key passphrase isn't stored here: settings files are plain text.">
  <Field key="env.NODE_EXTRA_CA_CERTS" type="env" label="Extra CA certificates" desc="Trust a company root certificate." placeholder="C:\certs\company-ca.pem" mono />
  <Field key="env.CLAUDE_CODE_CLIENT_CERT" type="env" label="Client certificate" desc="For mutual TLS." placeholder="C:\certs\client.pem" mono />
  <Field key="env.CLAUDE_CODE_CLIENT_KEY" type="env" label="Client key" desc="The certificate's private key." placeholder="C:\certs\client-key.pem" mono />
</Section>

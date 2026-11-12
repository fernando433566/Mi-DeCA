package es.deca.app;

import java.net.URI;

/** Valida destinos antes de adjuntar credenciales de la sesión. */
final class ServerUrl {
    private ServerUrl() {}

    static String authenticated(String server, String destination) {
        URI base = URI.create(server);
        URI target = URI.create(destination);
        int basePort = port(base), targetPort = port(target);
        if (base.getHost() == null || target.getHost() == null
                || !("https".equalsIgnoreCase(target.getScheme()) || "http".equalsIgnoreCase(target.getScheme()))
                || !base.getScheme().equalsIgnoreCase(target.getScheme())
                || !base.getHost().equalsIgnoreCase(target.getHost()) || basePort != targetPort
                || target.getRawUserInfo() != null || target.getRawFragment() != null) {
            throw new IllegalArgumentException("La descarga debe pertenecer al servidor de la sesión");
        }
        return target.toASCIIString();
    }

    private static int port(URI uri) {
        return uri.getPort() >= 0 ? uri.getPort() : "https".equalsIgnoreCase(uri.getScheme()) ? 443 : 80;
    }
}

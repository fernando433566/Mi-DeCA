// Validaciones sencillas en el cliente (el servidor vuelve a validarlo todo).

export function password(p1, p2) {
  if (!p1 || p1.length < 4) throw new Error('La contraseña debe tener al menos 4 caracteres');
  if (p2 !== undefined && p1 !== p2) throw new Error('Las dos contraseñas no coinciden');
}

export function required(value, label) {
  if (value === null || value === undefined || String(value).trim() === '') {
    throw new Error(`Falta rellenar: ${label}`);
  }
}

export function plate(p) {
  return String(p || '').replace(/[\s-]/g, '').toUpperCase();
}

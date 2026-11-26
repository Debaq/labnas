/** Nombre y unidad de cada tipo de lectura de los sensores */
export const READING_LABELS: Record<string, string> = {
  temperature: 'Temperatura', humidity: 'Humedad', pressure: 'Presion',
  co2: 'CO2', tvoc: 'TVOC', pm2_5: 'PM2.5', pm10: 'PM10',
  light: 'Luz', uv_index: 'UV', noise: 'Ruido',
  soil_moisture: 'Humedad suelo', water_level: 'Nivel agua', ph: 'pH',
  conductivity: 'Conductividad', dissolved_o2: 'O2 disuelto',
  wind_speed: 'Viento', wind_direction: 'Dir. viento', rain: 'Lluvia',
  weight: 'Peso', current: 'Corriente', voltage: 'Voltaje',
  power: 'Potencia', energy: 'Energia', distance: 'Distancia', flow_rate: 'Flujo',
}

export const READING_UNITS: Record<string, string> = {
  temperature: '°C', humidity: '%', pressure: 'hPa', co2: 'ppm', tvoc: 'ppb',
  pm2_5: 'ug/m3', pm10: 'ug/m3', light: 'lux', noise: 'dB',
  soil_moisture: '%', water_level: 'cm', conductivity: 'uS/cm', dissolved_o2: 'mg/L',
  wind_speed: 'm/s', wind_direction: '°', rain: 'mm', weight: 'g',
  current: 'A', voltage: 'V', power: 'W', energy: 'Wh', distance: 'cm', flow_rate: 'L/min',
}

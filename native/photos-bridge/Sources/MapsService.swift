import Foundation
import MapKit

/// Apple Maps place search. Returns raw candidates only; ranking happens in Rust.
enum MapsService {
    static func search(query: String, limit: Int) async throws -> [[String: Any]] {
        let request = MKLocalSearch.Request()
        request.naturalLanguageQuery = query
        request.resultTypes = [.pointOfInterest, .address, .physicalFeature]

        let response: MKLocalSearch.Response
        do {
            response = try await MKLocalSearch(request: request).start()
        } catch let error as MKError where error.code == .placemarkNotFound {
            return []
        } catch let error as MKError where error.code == .loadingThrottled {
            throw BridgeError.message("throttled")
        }

        return response.mapItems.prefix(limit).map { item in
            let reps = item.addressRepresentations
            let coordinate = item.location.coordinate
            var out: [String: Any] = [
                "name": item.name ?? reps?.cityName ?? query,
                "latitude": coordinate.latitude,
                "longitude": coordinate.longitude,
            ]
            out["address"] = item.address?.shortAddress ?? item.address?.fullAddress
            out["city"] = reps?.cityName
            out["region"] = reps?.cityWithContext
            out["country"] = reps?.regionName
            out["countryCode"] = reps?.region?.identifier
            out["mapIdentifier"] = item.identifier?.rawValue
            out["category"] = item.pointOfInterestCategory?.rawValue.replacingOccurrences(of: "MKPOICategory", with: "")
            return out.compactMapValues { $0 }
        }
    }
}
